#include <gst/gst.h>
#include <json-c/json.h>

#include <gstnvdsmeta.h>
#include <nvdsmeta.h>

#include <algorithm>
#include <atomic>
#include <chrono>
#include <climits>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <limits>
#include <memory>
#include <mutex>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <system_error>
#include <utility>
#include <vector>

#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

namespace {

constexpr std::string_view kRequestSchema =
    "veoveo.ai/stream-recording-runner-request/v2";
constexpr std::string_view kResponseSchema =
    "veoveo.ai/stream-recording-runner-response/v2";
constexpr std::string_view kLiveRequestSchema =
    "veoveo.ai/stream-live-runner-request/v2";
constexpr std::string_view kLiveFrameSchema =
    "veoveo.ai/stream-live-frame/v2";
constexpr std::string_view kLiveVideoChunkSchema =
    "veoveo.ai/stream-live-video-chunk/v2";
constexpr std::string_view kTrackerLibrary =
    "/opt/nvidia/deepstream/deepstream/lib/libnvds_nvmultiobjecttracker.so";

using JsonPtr = std::unique_ptr<json_object, decltype(&json_object_put)>;

struct IndexRange {
  std::int64_t start;
  std::int64_t end;
};

enum class SamplingMode { EveryFrame, EveryNth, MaximumFrames };

struct Sampling {
  SamplingMode mode;
  std::uint64_t value;
};

struct TrackerRequest {
  std::filesystem::path config_path;
  std::uint32_t width;
  std::uint32_t height;
};

struct Request {
  bool live = false;
  std::filesystem::path input_mp4;
  std::uint32_t input_width = 0;
  std::uint32_t input_height = 0;
  std::filesystem::path response_json;
  std::string launch;
  std::optional<std::string> source_element;
  std::optional<std::string> stream_muxer_element;
  std::optional<std::string> inference_element;
  std::optional<std::string> tracker_element;
  std::optional<std::string> results_element;
  std::optional<std::string> encoded_output_element;
  std::string operation;
  std::filesystem::path inference_config_path;
  std::optional<TrackerRequest> tracker;
  std::filesystem::path model_path;
  IndexRange requested_range;
  std::int64_t decode_start_index = 0;
  Sampling sampling;
  std::size_t max_output_frames = 0;
  std::size_t max_detections_per_frame = 0;
  std::size_t max_response_bytes = 0;
  std::size_t max_video_chunk_bytes = 0;
};

struct Bounds {
  double x;
  double y;
  double width;
  double height;
};

struct Detection {
  std::uint32_t class_id;
  std::string label;
  std::optional<double> confidence;
  std::optional<double> tracker_confidence;
  Bounds bounds;
  std::optional<std::uint64_t> track_id;
};

struct Frame {
  std::int64_t index;
  std::vector<Detection> detections;
};

[[noreturn]] void fail(const std::string &message);

json_object *frame_json(const Frame &frame) {
  auto *frame_json = json_object_new_object();
  json_object_object_add(frame_json, "index",
                         json_object_new_int64(frame.index));
  auto *detections = json_object_new_array_ext(static_cast<int>(
      std::min<std::size_t>(frame.detections.size(), INT_MAX)));
  for (const auto &detection : frame.detections) {
    auto *detection_json = json_object_new_object();
    json_object_object_add(detection_json, "classId",
                           json_object_new_uint64(detection.class_id));
    json_object_object_add(detection_json, "label",
                           json_object_new_string(detection.label.c_str()));
    if (detection.confidence) {
      json_object_object_add(detection_json, "confidence",
                             json_object_new_double(*detection.confidence));
    }
    if (detection.tracker_confidence) {
      json_object_object_add(
          detection_json, "trackerConfidence",
          json_object_new_double(*detection.tracker_confidence));
    }
    auto *bounds = json_object_new_object();
    json_object_object_add(bounds, "x",
                           json_object_new_double(detection.bounds.x));
    json_object_object_add(bounds, "y",
                           json_object_new_double(detection.bounds.y));
    json_object_object_add(bounds, "width",
                           json_object_new_double(detection.bounds.width));
    json_object_object_add(bounds, "height",
                           json_object_new_double(detection.bounds.height));
    json_object_object_add(detection_json, "bounds", bounds);
    if (detection.track_id) {
      json_object_object_add(detection_json, "trackId",
                             json_object_new_uint64(*detection.track_id));
    }
    json_object_array_add(detections, detection_json);
  }
  json_object_object_add(frame_json, "detections", detections);
  return frame_json;
}

class EventWriter {
public:
  explicit EventWriter(const std::filesystem::path &socket_path) {
    sockaddr_un address{};
    if (socket_path.string().size() >= sizeof(address.sun_path)) {
      fail("event socket path is too long");
    }
    fd_ = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (fd_ == -1) {
      fail("failed to create event socket");
    }
    address.sun_family = AF_UNIX;
    std::memcpy(address.sun_path, socket_path.c_str(),
                socket_path.string().size() + 1);
    if (connect(fd_, reinterpret_cast<sockaddr *>(&address),
                sizeof(address)) == -1) {
      close(fd_);
      fd_ = -1;
      fail("failed to connect to the Stream event socket");
    }
  }

  EventWriter(const EventWriter &) = delete;
  EventWriter &operator=(const EventWriter &) = delete;

  ~EventWriter() {
    if (fd_ != -1) {
      close(fd_);
    }
  }

  void write_frame(const Frame &frame) {
    JsonPtr root(json_object_new_object(), &json_object_put);
    json_object_object_add(root.get(), "schema",
                           json_object_new_string(kLiveFrameSchema.data()));
    json_object_object_add(root.get(), "frame", frame_json(frame));
    const char *serialized =
        json_object_to_json_string_ext(root.get(), JSON_C_TO_STRING_PLAIN);
    if (serialized == nullptr) {
      fail("failed to serialize live Stream frame");
    }
    std::lock_guard lock(mutex_);
    write_all(serialized, std::strlen(serialized));
    write_all("\n", 1);
  }

  void write_video_chunk(std::uint64_t sequence, std::uint64_t timestamp_us,
                         bool keyframe, const std::uint8_t *bytes,
                         std::size_t length) {
    gchar *encoded = g_base64_encode(bytes, length);
    if (encoded == nullptr) {
      fail("failed to encode live H.264 access unit");
    }
    JsonPtr root(json_object_new_object(), &json_object_put);
    json_object_object_add(
        root.get(), "schema",
        json_object_new_string(kLiveVideoChunkSchema.data()));
    auto *chunk = json_object_new_object();
    json_object_object_add(chunk, "sequence",
                           json_object_new_uint64(sequence));
    json_object_object_add(chunk, "timestampUs",
                           json_object_new_uint64(timestamp_us));
    json_object_object_add(chunk, "keyframe",
                           json_object_new_boolean(keyframe));
    json_object_object_add(chunk, "dataBase64",
                           json_object_new_string(encoded));
    g_free(encoded);
    json_object_object_add(root.get(), "chunk", chunk);
    const char *serialized =
        json_object_to_json_string_ext(root.get(), JSON_C_TO_STRING_PLAIN);
    if (serialized == nullptr) {
      fail("failed to serialize live H.264 access unit");
    }
    std::lock_guard lock(mutex_);
    write_all(serialized, std::strlen(serialized));
    write_all("\n", 1);
  }

private:
  void write_all(const char *bytes, std::size_t length) {
    std::size_t offset = 0;
    while (offset < length) {
      const auto written =
          send(fd_, bytes + offset, length - offset, MSG_NOSIGNAL);
      if (written <= 0) {
        fail("Stream event socket closed");
      }
      offset += static_cast<std::size_t>(written);
    }
  }

  int fd_ = -1;
  std::mutex mutex_;
};

struct ProbeContext {
  explicit ProbeContext(const Request &request, EventWriter *events = nullptr)
      : request(request), events(events) {}

  const Request &request;
  EventWriter *events;
  std::vector<Frame> frames;
  std::uint64_t processed_frames = 0;
  std::uint64_t eligible_ordinal = 0;
  std::uint64_t encoded_sequence = 0;
  std::size_t estimated_response_bytes = 128;
  std::optional<std::int64_t> last_index;
  std::atomic<GstElement *> pipeline{nullptr};
  std::mutex error_mutex;
  std::string error;

  void fail(std::string message) {
    bool first_failure = false;
    {
      std::lock_guard lock(error_mutex);
      if (error.empty()) {
        error = std::move(message);
        first_failure = true;
      }
    }
    auto *current_pipeline = pipeline.load(std::memory_order_acquire);
    if (first_failure && current_pipeline != nullptr) {
      gst_element_post_message(
          current_pipeline,
          gst_message_new_application(
              GST_OBJECT(current_pipeline),
              gst_structure_new_empty("veoveo-stream-probe-failure")));
    }
  }

  [[nodiscard]] std::string error_copy() {
    std::lock_guard lock(error_mutex);
    return error;
  }
};

class ProbePipelineBinding {
public:
  ProbePipelineBinding(ProbeContext &probe, GstElement *pipeline)
      : probe_(probe) {
    probe_.pipeline.store(pipeline, std::memory_order_release);
  }

  ProbePipelineBinding(const ProbePipelineBinding &) = delete;
  ProbePipelineBinding &operator=(const ProbePipelineBinding &) = delete;

  ~ProbePipelineBinding() {
    probe_.pipeline.store(nullptr, std::memory_order_release);
  }

private:
  ProbeContext &probe_;
};

[[noreturn]] void fail(const std::string &message) {
  throw std::runtime_error(message);
}

void redirect_native_stdout_to_stderr() {
  if (std::fflush(stdout) != 0 ||
      dup2(STDERR_FILENO, STDOUT_FILENO) == -1) {
    fail("failed to redirect native library stdout");
  }
}

json_object *required_member(json_object *object, const char *name,
                             json_type expected) {
  json_object *member = nullptr;
  if (!json_object_object_get_ex(object, name, &member)) {
    fail(std::string("missing JSON field `") + name + "`");
  }
  if (!json_object_is_type(member, expected)) {
    fail(std::string("JSON field `") + name + "` has the wrong type");
  }
  return member;
}

std::string required_string(json_object *object, const char *name) {
  auto *member = required_member(object, name, json_type_string);
  const char *value = json_object_get_string(member);
  if (value == nullptr || *value == '\0' || std::strlen(value) != static_cast<std::size_t>(json_object_get_string_len(member))) {
    fail(std::string("JSON field `") + name + "` must not be empty");
  }
  return value;
}

std::optional<std::string> optional_string(json_object *object,
                                           const char *name) {
  json_object *member = nullptr;
  if (!json_object_object_get_ex(object, name, &member) ||
      json_object_is_type(member, json_type_null)) {
    return std::nullopt;
  }
  if (!json_object_is_type(member, json_type_string)) {
    fail(std::string("JSON field `") + name + "` has the wrong type");
  }
  const char *value = json_object_get_string(member);
  if (value == nullptr || *value == '\0' || std::strlen(value) != static_cast<std::size_t>(json_object_get_string_len(member))) {
    fail(std::string("JSON field `") + name + "` must not be empty");
  }
  return std::string(value);
}

void require_catalog_id(json_object *object, const char *name) {
  const auto value = required_string(object, name);
  const auto admitted = [](unsigned char c) { return (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9'); };
  if (value.size() > 128 || !admitted(value.front()) || !std::all_of(value.begin(), value.end(), [&](unsigned char c) { return admitted(c) || c == '-'; })) {
    fail("invalid Stream catalog identity");
  }
}

void require_uuid_v7(json_object *object, const char *name) {
  const auto value = required_string(object, name);
  if (value.size() != 36 || value[14] != '7' || std::string_view("89ab").find(value[19]) == std::string_view::npos) {
    fail("invalid Stream UUIDv7 identity");
  }
  for (std::size_t index = 0; index < value.size(); ++index) {
    const auto c = value[index];
    const bool separator = index == 8 || index == 13 || index == 18 || index == 23;
    if (separator ? c != '-' : !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) {
      fail("invalid Stream UUIDv7 identity");
    }
  }
}

std::int64_t required_i64(json_object *object, const char *name) {
  auto *member = required_member(object, name, json_type_int);
  if (json_object_get_uint64(member) > static_cast<std::uint64_t>(INT64_MAX)) {
    fail("JSON integer exceeds i64");
  }
  return json_object_get_int64(member);
}

std::uint64_t required_positive_u64(json_object *object, const char *name) {
  const auto value = required_i64(object, name);
  if (value <= 0) {
    fail(std::string("JSON field `") + name + "` must be positive");
  }
  return static_cast<std::uint64_t>(value);
}

std::uint32_t required_positive_u32(json_object *object, const char *name) {
  const auto value = required_positive_u64(object, name);
  if (value > std::numeric_limits<std::uint32_t>::max()) {
    fail(std::string("JSON field `") + name + "` exceeds u32");
  }
  return static_cast<std::uint32_t>(value);
}

std::size_t required_positive_size(json_object *object, const char *name) {
  const auto value = required_positive_u64(object, name);
  if (value > std::numeric_limits<std::size_t>::max()) {
    fail(std::string("JSON field `") + name + "` exceeds size_t");
  }
  return static_cast<std::size_t>(value);
}

std::filesystem::path required_absolute_path(json_object *object,
                                             const char *name) {
  std::filesystem::path path(required_string(object, name));
  if (!path.is_absolute()) {
    fail(std::string("JSON field `") + name + "` must be an absolute path");
  }
  return path;
}

void require_regular_file(const std::filesystem::path &path,
                          std::string_view description) {
  std::error_code error;
  if (!std::filesystem::is_regular_file(path, error) || error) {
    fail(std::string(description) +
         " is not a readable regular file");
  }
}

void closed_object(json_object *object, std::initializer_list<std::string_view> fields) {
  if (!json_object_is_type(object, json_type_object)) {
    fail("expected a JSON object");
  }
  json_object_object_foreach(object, key, value) {
    (void)value;
    if (std::find(fields.begin(), fields.end(), std::string_view(key)) == fields.end()) {
      fail("unknown JSON field");
    }
  }
}

Sampling parse_sampling(json_object *object) {
  const auto mode = required_string(object, "mode");
  if (mode == "every_frame") {
    closed_object(object, {"mode"});
    return {SamplingMode::EveryFrame, 1};
  }
  if (mode == "every_nth") {
    closed_object(object, {"mode", "step"});
    return {SamplingMode::EveryNth, required_positive_u64(object, "step")};
  }
  if (mode == "maximum_frames") {
    closed_object(object, {"mode", "count"});
    return {SamplingMode::MaximumFrames,
            required_positive_u64(object, "count")};
  }
  fail("unsupported sampling mode");
}

void parse_pipeline_and_model(json_object *root, Request &request) {
  auto *pipeline = required_member(root, "pipeline", json_type_object);
  closed_object(pipeline, {"pipelineId", "graph", "profile"});
  require_catalog_id(pipeline, "pipelineId");
  auto *graph = required_member(pipeline, "graph", json_type_object);
  closed_object(graph, {"launch", "sourceElement", "streamMuxerElement", "inferenceElement", "trackerElement", "resultsElement", "encodedOutputElement"});
  request.launch = required_string(graph, "launch");
  if (request.launch.size() > 64U * 1024U ||
      request.launch.find('\0') != std::string::npos) {
    fail("GStreamer launch text exceeds the admitted bound");
  }
  request.source_element = optional_string(graph, "sourceElement");
  request.stream_muxer_element =
      optional_string(graph, "streamMuxerElement");
  request.inference_element = optional_string(graph, "inferenceElement");
  request.tracker_element = optional_string(graph, "trackerElement");
  request.results_element = optional_string(graph, "resultsElement");
  request.encoded_output_element =
      optional_string(graph, "encodedOutputElement");

  auto *profile = required_member(pipeline, "profile", json_type_object);
  const auto profile_kind = required_string(profile, "kind");
  if (profile_kind == "pass_through") {
    closed_object(profile, {"kind"});
    request.operation = profile_kind;
    if (request.stream_muxer_element || request.inference_element ||
        request.tracker_element || request.results_element) {
      fail("pass-through profile must not declare inference result elements");
    }
    json_object *model = nullptr;
    if (json_object_object_get_ex(root, "model", &model) &&
        !json_object_is_type(model, json_type_null)) {
      fail("pass-through profile must not declare a model");
    }
    return;
  }
  if (profile_kind != "perception") {
    fail("unsupported Stream pipeline profile");
  }
  closed_object(profile, {"kind", "operation", "inferenceConfigPath", "tracker"});
  request.operation = required_string(profile, "operation");
  if (request.operation != "object_detection" &&
      request.operation != "object_detection_tracking") {
    fail("the typed perception profile supports only object detection");
  }
  request.inference_config_path =
      required_absolute_path(profile, "inferenceConfigPath");

  json_object *tracker = nullptr;
  if (json_object_object_get_ex(profile, "tracker", &tracker) &&
      !json_object_is_type(tracker, json_type_null)) {
    if (!json_object_is_type(tracker, json_type_object)) {
      fail("JSON field `tracker` has the wrong type");
    }
    closed_object(tracker, {"configPath", "width", "height"});
    TrackerRequest parsed;
    parsed.config_path = required_absolute_path(tracker, "configPath");
    parsed.width = required_positive_u32(tracker, "width");
    parsed.height = required_positive_u32(tracker, "height");
    if (parsed.width % 32 != 0 || parsed.height % 32 != 0) {
      fail("tracker dimensions must be multiples of 32");
    }
    request.tracker = std::move(parsed);
  }
  if ((request.operation == "object_detection_tracking") !=
      request.tracker.has_value()) {
    fail("tracker presence does not match the selected pipeline operation");
  }
  if (request.tracker.has_value() != request.tracker_element.has_value()) {
    fail("tracker element presence does not match the selected pipeline operation");
  }
  if (!request.stream_muxer_element || !request.inference_element ||
      !request.results_element) {
    fail("perception profile requires stream muxer, inference, and results elements");
  }

  auto *model = required_member(root, "model", json_type_object);
  closed_object(model, {"modelId", "modelPath", "format"});
  require_catalog_id(model, "modelId");
  if (required_string(model, "format") != "tensor_rt_engine") {
    fail("the typed perception profile accepts TensorRT engine models only");
  }
  request.model_path = required_absolute_path(model, "modelPath");
}

// The current private JSON envelope is admitted before any GStreamer or socket effects.
JsonPtr read_request(const std::filesystem::path &path) {
  constexpr std::size_t kMaximumRequestBytes = 1024U * 1024U;
  std::ifstream input(path, std::ios::binary);
  if (!input) { fail("failed to read request JSON"); }
  std::string bytes(kMaximumRequestBytes + 1, '\0');
  input.read(bytes.data(), static_cast<std::streamsize>(bytes.size()));
  const auto count = static_cast<std::size_t>(input.gcount());
  if (count == 0 || count > kMaximumRequestBytes || input.bad()) {
    fail("request JSON exceeds the admitted byte bound or cannot be read");
  }
  bytes.resize(count);
  std::unique_ptr<json_tokener, decltype(&json_tokener_free)> tokener(json_tokener_new_ex(64), &json_tokener_free);
  if (!tokener) { fail("failed to allocate request decoder"); }
  json_tokener_set_flags(tokener.get(), JSON_TOKENER_STRICT | JSON_TOKENER_VALIDATE_UTF8);
  JsonPtr result(json_tokener_parse_ex(tokener.get(), bytes.data(), static_cast<int>(bytes.size())), &json_object_put);
  if (json_tokener_get_error(tokener.get()) != json_tokener_success || !result) {
    fail("request JSON is invalid");
  }
  const auto end = json_tokener_get_parse_end(tokener.get());
  if (end > bytes.size() || bytes.find_first_not_of(" \t\r\n", end) != std::string::npos) {
    fail("request JSON has trailing data");
  }
  return result;
}

Request parse_request(const std::filesystem::path &request_path,
                      const std::filesystem::path &cli_response_path) {
  auto root = read_request(request_path);
  if (!root || !json_object_is_type(root.get(), json_type_object)) {
    fail("request JSON is not a valid object");
  }
  if (required_string(root.get(), "schema") != kRequestSchema) {
    fail("unsupported Stream recording runner request schema");
  }
  closed_object(root.get(), {"schema", "taskId", "inputMp4", "inputWidth", "inputHeight", "responseJson", "pipeline", "model", "requestedRange", "decodeStartIndex", "sampling", "maxOutputFrames", "maxDetectionsPerFrame", "maxResponseBytes"});
  require_uuid_v7(root.get(), "taskId");

  Request request;
  request.input_mp4 = required_absolute_path(root.get(), "inputMp4");
  request.input_width = required_positive_u32(root.get(), "inputWidth");
  request.input_height = required_positive_u32(root.get(), "inputHeight");
  if (request.input_width > UINT16_MAX || request.input_height > UINT16_MAX) { fail("input dimensions exceed u16"); }
  request.response_json = required_absolute_path(root.get(), "responseJson");
  if (request.response_json != cli_response_path) {
    fail("response path argument does not match the typed request");
  }

  parse_pipeline_and_model(root.get(), request);
  if (request.operation == "pass_through") {
    fail("recording replay does not support pass-through profiles");
  }

  auto *range =
      required_member(root.get(), "requestedRange", json_type_object);
  closed_object(range, {"start", "end"});
  request.requested_range = {required_i64(range, "start"),
                             required_i64(range, "end")};
  if (request.requested_range.start > request.requested_range.end) {
    fail("requested range is reversed");
  }
  request.decode_start_index = required_i64(root.get(), "decodeStartIndex");
  if (request.decode_start_index > request.requested_range.end) {
    fail("decode start is after the requested range end");
  }
  request.sampling =
      parse_sampling(required_member(root.get(), "sampling", json_type_object));
  request.max_output_frames =
      required_positive_size(root.get(), "maxOutputFrames");
  request.max_detections_per_frame =
      required_positive_size(root.get(), "maxDetectionsPerFrame");
  request.max_response_bytes =
      required_positive_size(root.get(), "maxResponseBytes");
  if (request.sampling.mode == SamplingMode::MaximumFrames &&
      request.sampling.value > request.max_output_frames) {
    fail("maximum_frames count exceeds max_output_frames");
  }

  require_regular_file(request.input_mp4, "input MP4");
  if (request.operation != "pass_through") {
    require_regular_file(request.inference_config_path,
                         "DeepStream inference config");
    require_regular_file(request.model_path, "TensorRT engine");
    if (request.tracker) {
      require_regular_file(request.tracker->config_path, "tracker config");
      require_regular_file(std::filesystem::path(std::string(kTrackerLibrary)),
                           "DeepStream tracker library");
    }
  }
  return request;
}

Request parse_live_request(const std::filesystem::path &request_path) {
  auto root = read_request(request_path);
  if (!root || !json_object_is_type(root.get(), json_type_object)) {
    fail("live request JSON is not a valid object");
  }
  if (required_string(root.get(), "schema") != kLiveRequestSchema) {
    fail("unsupported Stream live runner request schema");
  }
  closed_object(root.get(), {"schema", "sessionId", "inputWidth", "inputHeight", "pipeline", "model", "maxDetectionsPerFrame", "maxEventBytes", "maxVideoChunkBytes"});
  require_uuid_v7(root.get(), "sessionId");

  Request request;
  request.live = true;
  request.input_width = required_positive_u32(root.get(), "inputWidth");
  request.input_height = required_positive_u32(root.get(), "inputHeight");
  if (request.input_width > UINT16_MAX || request.input_height > UINT16_MAX) { fail("input dimensions exceed u16"); }
  parse_pipeline_and_model(root.get(), request);
  request.requested_range = {
      std::numeric_limits<std::int64_t>::min(),
      std::numeric_limits<std::int64_t>::max(),
  };
  request.decode_start_index = 0;
  request.sampling = {SamplingMode::EveryFrame, 1};
  request.max_output_frames = 1;
  request.max_detections_per_frame =
      required_positive_size(root.get(), "maxDetectionsPerFrame");
  request.max_response_bytes =
      required_positive_size(root.get(), "maxEventBytes");
  request.max_video_chunk_bytes =
      required_positive_size(root.get(), "maxVideoChunkBytes");

  if (request.operation != "pass_through") {
    require_regular_file(request.inference_config_path,
                         "DeepStream inference config");
    require_regular_file(request.model_path, "TensorRT engine");
    if (request.tracker) {
      require_regular_file(request.tracker->config_path, "tracker config");
      require_regular_file(std::filesystem::path(std::string(kTrackerLibrary)),
                           "DeepStream tracker library");
    }
  }
  return request;
}

bool should_emit(ProbeContext &context) {
  const auto ordinal = context.eligible_ordinal++;
  switch (context.request.sampling.mode) {
  case SamplingMode::EveryFrame:
    return true;
  case SamplingMode::EveryNth:
    return ordinal % context.request.sampling.value == 0;
  case SamplingMode::MaximumFrames:
    return context.frames.size() < context.request.sampling.value;
  }
  return false;
}

std::optional<double> optional_confidence(float value, std::string_view name,
                                          ProbeContext &context) {
  if (value < 0.0F) {
    return std::nullopt;
  }
  if (!std::isfinite(value) || value > 1.0F) {
    context.fail(std::string(name) + " is outside 0..=1");
    return std::nullopt;
  }
  return static_cast<double>(value);
}

GstPadProbeReturn inference_probe(GstPad *, GstPadProbeInfo *info,
                                  gpointer user_data) {
  auto &context = *static_cast<ProbeContext *>(user_data);
  if (!context.error_copy().empty()) {
    return GST_PAD_PROBE_OK;
  }
  auto *buffer = GST_PAD_PROBE_INFO_BUFFER(info);
  if (buffer == nullptr) {
    return GST_PAD_PROBE_OK;
  }
  auto *batch = gst_buffer_get_nvds_batch_meta(buffer);
  if (batch == nullptr) {
    context.fail("DeepStream buffer is missing NvDsBatchMeta");
    return GST_PAD_PROBE_OK;
  }

  for (auto *frame_node = batch->frame_meta_list; frame_node != nullptr;
       frame_node = frame_node->next) {
    auto *frame_meta = static_cast<NvDsFrameMeta *>(frame_node->data);
    if (frame_meta == nullptr) {
      context.fail("DeepStream batch contains null frame metadata");
      continue;
    }
    std::int64_t index = 0;
    if (context.request.live) {
      const auto maximum_live_sequence = static_cast<std::uint64_t>(
          std::numeric_limits<std::int64_t>::max());
      if (context.processed_frames > maximum_live_sequence) {
        context.fail("live decoded-frame sequence overflowed i64");
        continue;
      }
      index = static_cast<std::int64_t>(context.processed_frames);
      ++context.processed_frames;
    } else {
      if (frame_meta->buf_pts == GST_CLOCK_TIME_NONE ||
          frame_meta->buf_pts > static_cast<std::uint64_t>(
                                    std::numeric_limits<std::int64_t>::max())) {
        context.fail("decoded frame has no representable presentation timestamp");
        continue;
      }
      const auto pts = static_cast<std::int64_t>(frame_meta->buf_pts);
      if (pts > 0 && context.request.decode_start_index >
                         std::numeric_limits<std::int64_t>::max() - pts) {
        context.fail("decoded frame index overflowed i64");
        continue;
      }
      index = context.request.decode_start_index + pts;
      if (index < context.request.requested_range.start ||
          index > context.request.requested_range.end) {
        continue;
      }
      ++context.processed_frames;
    }
    if (!should_emit(context)) {
      continue;
    }
    if (!context.request.live &&
        context.frames.size() >= context.request.max_output_frames) {
      context.fail("DeepStream output exceeded max_output_frames");
      continue;
    }
    if (context.request.live) {
      context.estimated_response_bytes = 128;
    }

    if (context.estimated_response_bytes >
        context.request.max_response_bytes -
            std::min<std::size_t>(context.request.max_response_bytes, 64)) {
      context.fail("DeepStream output exceeded max_response_bytes");
      continue;
    }
    context.estimated_response_bytes += 64;
    Frame frame{index, {}};
    frame.detections.reserve(std::min<std::size_t>(
        frame_meta->num_obj_meta, context.request.max_detections_per_frame));
    for (auto *object_node = frame_meta->obj_meta_list; object_node != nullptr;
         object_node = object_node->next) {
      if (frame.detections.size() >= context.request.max_detections_per_frame) {
        context.fail("DeepStream output exceeded max_detections_per_frame");
        break;
      }
      auto *object = static_cast<NvDsObjectMeta *>(object_node->data);
      if (object == nullptr || object->class_id < 0) {
        context.fail("DeepStream returned invalid object metadata");
        continue;
      }
      const auto &rect = object->rect_params;
      if (!std::isfinite(rect.left) || !std::isfinite(rect.top) ||
          !std::isfinite(rect.width) || !std::isfinite(rect.height)) {
        context.fail("DeepStream returned non-finite object bounds");
        continue;
      }
      const auto max_width =
          static_cast<double>(frame_meta->source_frame_width);
      const auto max_height =
          static_cast<double>(frame_meta->source_frame_height);
      const auto left =
          std::clamp(static_cast<double>(rect.left), 0.0, max_width);
      const auto top =
          std::clamp(static_cast<double>(rect.top), 0.0, max_height);
      const auto right = std::clamp(static_cast<double>(rect.left + rect.width),
                                    left, max_width);
      const auto bottom = std::clamp(
          static_cast<double>(rect.top + rect.height), top, max_height);
      if (right <= left || bottom <= top) {
        context.fail("DeepStream returned empty object bounds");
        continue;
      }
      const auto label_length = strnlen(object->obj_label, MAX_LABEL_SIZE);
      if (label_length == 0 || label_length == MAX_LABEL_SIZE) {
        context.fail(
            "DeepStream returned an empty or unterminated object label");
        continue;
      }
      const auto detection_estimate =
          static_cast<std::size_t>(256) + label_length;
      if (detection_estimate > context.request.max_response_bytes ||
          context.estimated_response_bytes >
              context.request.max_response_bytes - detection_estimate) {
        context.fail("DeepStream output exceeded max_response_bytes");
        break;
      }
      context.estimated_response_bytes += detection_estimate;
      Detection detection{static_cast<std::uint32_t>(object->class_id),
                          std::string(object->obj_label, label_length),
                          optional_confidence(object->confidence,
                                              "detector confidence", context),
                          optional_confidence(object->tracker_confidence,
                                              "tracker confidence", context),
                          {left, top, right - left, bottom - top},
                          std::nullopt};
      if (object->object_id != UNTRACKED_OBJECT_ID) {
        detection.track_id = object->object_id;
      }
      frame.detections.push_back(std::move(detection));
    }
    if (context.last_index && *context.last_index >= frame.index) {
      context.fail("DeepStream frame indices are not strictly increasing");
      continue;
    }
    context.last_index = frame.index;
    if (context.request.live) {
      if (context.events == nullptr) {
        context.fail("live Stream runner has no event writer");
        continue;
      }
      try {
        context.events->write_frame(frame);
      } catch (const std::exception &error) {
        context.fail(error.what());
      }
    } else {
      context.frames.push_back(std::move(frame));
    }
  }
  return GST_PAD_PROBE_OK;
}

GstPadProbeReturn encoded_output_probe(GstPad *, GstPadProbeInfo *info,
                                       gpointer user_data) {
  auto &context = *static_cast<ProbeContext *>(user_data);
  if (!context.error_copy().empty()) {
    return GST_PAD_PROBE_OK;
  }
  auto *buffer = GST_PAD_PROBE_INFO_BUFFER(info);
  if (buffer == nullptr) {
    return GST_PAD_PROBE_OK;
  }
  if (context.events == nullptr) {
    context.fail("live Stream runner has no encoded-output event writer");
    return GST_PAD_PROBE_OK;
  }
  const auto timestamp = GST_BUFFER_PTS_IS_VALID(buffer)
                             ? GST_BUFFER_PTS(buffer)
                             : GST_BUFFER_DTS(buffer);
  if (timestamp == GST_CLOCK_TIME_NONE) {
    context.fail("live H.264 access unit has no timestamp");
    return GST_PAD_PROBE_OK;
  }
  GstMapInfo mapping{};
  if (!gst_buffer_map(buffer, &mapping, GST_MAP_READ)) {
    context.fail("failed to map live H.264 access unit");
    return GST_PAD_PROBE_OK;
  }
  if (mapping.size == 0 ||
      mapping.size > context.request.max_video_chunk_bytes) {
    gst_buffer_unmap(buffer, &mapping);
    context.fail("live H.264 access unit exceeds max_video_chunk_bytes");
    return GST_PAD_PROBE_OK;
  }
  const bool keyframe =
      !GST_BUFFER_FLAG_IS_SET(buffer, GST_BUFFER_FLAG_DELTA_UNIT);
  try {
    context.events->write_video_chunk(
        context.encoded_sequence++, timestamp / GST_USECOND, keyframe,
        mapping.data, mapping.size);
  } catch (const std::exception &error) {
    context.fail(error.what());
  }
  gst_buffer_unmap(buffer, &mapping);
  return GST_PAD_PROBE_OK;
}

GstElement *required_named_element(GstElement *pipeline,
                                   const std::string &name) {
  if (!GST_IS_BIN(pipeline)) {
    fail("admitted GStreamer launch did not create a bin");
  }
  auto *element = gst_bin_get_by_name(GST_BIN(pipeline), name.c_str());
  if (element == nullptr) {
    fail("admitted GStreamer launch omitted named element `" + name + "`");
  }
  return element;
}

void run_pipeline(const Request &request, ProbeContext &probe) {
  GError *parse_error = nullptr;
  GstElement *pipeline =
      gst_parse_launch_full(request.launch.c_str(), nullptr,
                            GST_PARSE_FLAG_FATAL_ERRORS, &parse_error);
  if (pipeline == nullptr || parse_error != nullptr) {
    std::string detail =
        parse_error == nullptr ? "unknown parse failure" : parse_error->message;
    if (parse_error != nullptr) {
      g_error_free(parse_error);
    }
    if (pipeline != nullptr) {
      gst_object_unref(pipeline);
    }
    fail("admitted GStreamer launch is invalid: " + detail);
  }
  auto pipeline_guard = std::unique_ptr<GstElement, void (*)(GstElement *)>(
      pipeline, [](GstElement *value) {
        gst_element_set_state(value, GST_STATE_NULL);
        gst_object_unref(value);
      });
  ProbePipelineBinding probe_pipeline(probe, pipeline);

  if (!request.source_element) {
    fail("admitted GStreamer graph omitted source_element");
  }
  auto *source = required_named_element(pipeline, *request.source_element);
  GstElement *mux =
      request.stream_muxer_element
          ? required_named_element(pipeline, *request.stream_muxer_element)
          : nullptr;
  GstElement *inference =
      request.inference_element
          ? required_named_element(pipeline, *request.inference_element)
          : nullptr;
  GstElement *results =
      request.results_element
          ? required_named_element(pipeline, *request.results_element)
          : nullptr;
  GstElement *tracker = request.tracker_element
                            ? required_named_element(pipeline,
                                                     *request.tracker_element)
                            : nullptr;
  GstElement *encoded_output =
      request.encoded_output_element
          ? required_named_element(pipeline, *request.encoded_output_element)
          : nullptr;

  if (!request.live) {
    g_object_set(source, "location", request.input_mp4.c_str(), nullptr);
  }
  if (mux != nullptr) {
    g_object_set(mux, "batch-size", 1U, "width", request.input_width, "height",
                 request.input_height, "live-source", request.live,
                 "batched-push-timeout", 40000, nullptr);
  }
  if (inference != nullptr) {
    g_object_set(inference, "config-file-path",
                 request.inference_config_path.c_str(), "model-engine-file",
                 request.model_path.c_str(), "batch-size", 1U, "process-mode",
                 1U, "interval", 0U, nullptr);
  }
  if (tracker != nullptr) {
    g_object_set(tracker, "ll-lib-file", kTrackerLibrary.data(),
                 "ll-config-file", request.tracker->config_path.c_str(),
                 "tracker-width", request.tracker->width, "tracker-height",
                 request.tracker->height, nullptr);
  }

  if (results != nullptr) {
    GstPad *sink_pad = gst_element_get_static_pad(results, "sink");
    if (sink_pad == nullptr) {
      fail("named results element has no static sink pad");
    }
    gst_pad_add_probe(sink_pad, GST_PAD_PROBE_TYPE_BUFFER, inference_probe,
                      &probe, nullptr);
    gst_object_unref(sink_pad);
  }
  if (request.live && encoded_output == nullptr) {
    fail("live pipeline omitted its encoded output element");
  }
  if (encoded_output != nullptr) {
    GstPad *encoded_pad = gst_element_get_static_pad(encoded_output, "src");
    if (encoded_pad == nullptr) {
      fail("named encoded output element has no static source pad");
    }
    gst_pad_add_probe(encoded_pad, GST_PAD_PROBE_TYPE_BUFFER,
                      encoded_output_probe, &probe, nullptr);
    gst_object_unref(encoded_pad);
  }
  gst_object_unref(source);
  if (mux != nullptr) {
    gst_object_unref(mux);
  }
  if (inference != nullptr) {
    gst_object_unref(inference);
  }
  if (results != nullptr) {
    gst_object_unref(results);
  }
  if (tracker != nullptr) {
    gst_object_unref(tracker);
  }
  if (encoded_output != nullptr) {
    gst_object_unref(encoded_output);
  }

  if (gst_element_set_state(pipeline, GST_STATE_PLAYING) ==
      GST_STATE_CHANGE_FAILURE) {
    fail("DeepStream pipeline refused the PLAYING state");
  }
  GstBus *bus = gst_element_get_bus(pipeline);
  GstMessage *message = nullptr;
  while (message == nullptr) {
    auto *candidate = gst_bus_timed_pop_filtered(
        bus, GST_CLOCK_TIME_NONE,
        static_cast<GstMessageType>(GST_MESSAGE_ERROR | GST_MESSAGE_EOS |
                                    GST_MESSAGE_APPLICATION));
    if (candidate == nullptr) {
      break;
    }
    if (GST_MESSAGE_TYPE(candidate) == GST_MESSAGE_APPLICATION &&
        probe.error_copy().empty()) {
      gst_message_unref(candidate);
      continue;
    }
    message = candidate;
  }
  gst_object_unref(bus);
  if (message == nullptr) {
    fail("DeepStream pipeline ended without EOS or an error");
  }
  if (GST_MESSAGE_TYPE(message) == GST_MESSAGE_ERROR) {
    GError *error = nullptr;
    gchar *debug = nullptr;
    gst_message_parse_error(message, &error, &debug);
    std::string detail =
        error == nullptr ? "unknown GStreamer error" : error->message;
    if (debug != nullptr && *debug != '\0') {
      detail += " (" + std::string(debug) + ")";
    }
    if (error != nullptr) {
      g_error_free(error);
    }
    g_free(debug);
    gst_message_unref(message);
    fail("DeepStream pipeline failed: " + detail);
  }
  gst_message_unref(message);
  const auto probe_error = probe.error_copy();
  if (!probe_error.empty()) {
    fail(probe_error);
  }
  if (!request.live && probe.processed_frames == 0) {
    fail("DeepStream decoded no frames inside the requested Rerun range");
  }
}

void add_optional_double(json_object *object, const char *name,
                         const std::optional<double> &value) {
  if (value) {
    json_object_object_add(object, name, json_object_new_double(*value));
  }
}

void write_response(const Request &request, const ProbeContext &probe,
                    std::uint64_t elapsed_ms) {
  JsonPtr root(json_object_new_object(), &json_object_put);
  json_object_object_add(root.get(), "schema",
                         json_object_new_string(kResponseSchema.data()));
  auto *frames = json_object_new_array_ext(
      static_cast<int>(std::min<std::size_t>(probe.frames.size(), INT_MAX)));
  for (const auto &frame : probe.frames) {
    auto *frame_json = json_object_new_object();
    json_object_object_add(frame_json, "index",
                           json_object_new_int64(frame.index));
    auto *detections = json_object_new_array_ext(static_cast<int>(
        std::min<std::size_t>(frame.detections.size(), INT_MAX)));
    for (const auto &detection : frame.detections) {
      auto *detection_json = json_object_new_object();
      json_object_object_add(detection_json, "classId",
                             json_object_new_uint64(detection.class_id));
      json_object_object_add(detection_json, "label",
                             json_object_new_string(detection.label.c_str()));
      add_optional_double(detection_json, "confidence", detection.confidence);
      add_optional_double(detection_json, "trackerConfidence",
                          detection.tracker_confidence);
      auto *bounds = json_object_new_object();
      json_object_object_add(bounds, "x",
                             json_object_new_double(detection.bounds.x));
      json_object_object_add(bounds, "y",
                             json_object_new_double(detection.bounds.y));
      json_object_object_add(bounds, "width",
                             json_object_new_double(detection.bounds.width));
      json_object_object_add(bounds, "height",
                             json_object_new_double(detection.bounds.height));
      json_object_object_add(detection_json, "bounds", bounds);
      if (detection.track_id) {
        json_object_object_add(detection_json, "trackId",
                               json_object_new_uint64(*detection.track_id));
      }
      json_object_array_add(detections, detection_json);
    }
    json_object_object_add(frame_json, "detections", detections);
    json_object_array_add(frames, frame_json);
  }
  json_object_object_add(root.get(), "frames", frames);
  json_object_object_add(root.get(), "processedFrames",
                         json_object_new_uint64(probe.processed_frames));
  json_object_object_add(root.get(), "elapsedMs",
                         json_object_new_uint64(elapsed_ms));

  const auto temporary = request.response_json.string() + ".tmp";
  const char *serialized =
      json_object_to_json_string_ext(root.get(), JSON_C_TO_STRING_PRETTY);
  if (serialized == nullptr) {
    fail("failed to serialize typed runner response");
  }
  const auto serialized_size = std::strlen(serialized);
  if (serialized_size > request.max_response_bytes) {
    fail("typed runner response exceeds max_response_bytes");
  }
  std::ofstream output(temporary, std::ios::binary | std::ios::trunc);
  if (!output) {
    fail("failed to create typed runner response");
  }
  output.write(serialized, static_cast<std::streamsize>(serialized_size));
  output.flush();
  if (!output) {
    fail("failed to write typed runner response");
  }
  output.close();
  std::error_code error;
  std::filesystem::rename(temporary, request.response_json, error);
  if (error) {
    std::filesystem::remove(temporary);
    fail("failed to publish typed runner response: " + error.message());
  }
}

struct CliArguments {
  std::filesystem::path request;
  std::optional<std::filesystem::path> response;
  std::optional<std::filesystem::path> event_socket;
  bool validate_request = false;
};

CliArguments parse_arguments(int argc, char **argv) {
  std::optional<std::filesystem::path> request;
  std::optional<std::filesystem::path> response;
  std::optional<std::filesystem::path> event_socket;
  bool validate_request = false;
  for (int index = 1; index < argc; ++index) {
    const std::string_view argument(argv[index]);
    if ((argument == "--request-json" || argument == "--response-json" ||
         argument == "--event-socket") &&
        index + 1 >= argc) {
      fail(std::string(argument) + " requires a path");
    }
    if (argument == "--validate-request") {
      if (validate_request) { fail("duplicate validation argument"); }
      validate_request = true;
    } else if (argument == "--request-json") {
      if (request) { fail("duplicate request argument"); }
      request = std::filesystem::path(argv[++index]);
    } else if (argument == "--response-json") {
      if (response) { fail("duplicate response argument"); }
      response = std::filesystem::path(argv[++index]);
    } else if (argument == "--event-socket") {
      if (event_socket) { fail("duplicate event socket argument"); }
      event_socket = std::filesystem::path(argv[++index]);
    } else {
      fail("unknown runner argument");
    }
  }
  if (!request || !request->is_absolute() ||
      (response.has_value() == event_socket.has_value()) ||
      (response && !response->is_absolute()) ||
      (event_socket && !event_socket->is_absolute())) {
    fail("--request-json and exactly one absolute --response-json or --event-socket path are required");
  }
  return {*request, response, event_socket, validate_request};
}

} // namespace

int main(int argc, char **argv) {
  try {
    const auto arguments = parse_arguments(argc, argv);
    const auto request = arguments.response
                             ? parse_request(arguments.request,
                                             *arguments.response)
                             : parse_live_request(arguments.request);
    if (arguments.validate_request) {
      std::fprintf(stderr, "stream-gst-runner: request admitted\n");
      return 0;
    }
    redirect_native_stdout_to_stderr();
    gst_init(nullptr, nullptr);
    std::unique_ptr<EventWriter> events;
    if (arguments.event_socket) {
      events = std::make_unique<EventWriter>(*arguments.event_socket);
    }
    ProbeContext probe(request, events.get());
    const auto started = std::chrono::steady_clock::now();
    run_pipeline(request, probe);
    const auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
                             std::chrono::steady_clock::now() - started)
                             .count();
    if (arguments.response) {
      write_response(request, probe, static_cast<std::uint64_t>(elapsed));
    }
    return 0;
  } catch (const std::exception &error) {
    std::fprintf(stderr, "stream-gst-runner: %s\n", error.what());
    return 1;
  }
}

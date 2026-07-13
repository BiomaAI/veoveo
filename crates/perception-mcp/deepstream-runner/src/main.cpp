#include <gst/gst.h>
#include <json-c/json.h>

#include <gstnvdsmeta.h>
#include <nvdsmeta.h>

#include <algorithm>
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

#include <unistd.h>

namespace {

constexpr std::string_view kRequestSchema =
    "veoveo.perception-runner-request/v1";
constexpr std::string_view kResponseSchema =
    "veoveo.perception-runner-response/v1";
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
  std::filesystem::path input_mp4;
  std::uint32_t input_width;
  std::uint32_t input_height;
  std::filesystem::path response_json;
  std::string operation;
  std::filesystem::path inference_config_path;
  std::optional<TrackerRequest> tracker;
  std::filesystem::path model_path;
  IndexRange requested_range;
  std::int64_t decode_start_index;
  Sampling sampling;
  std::size_t max_output_frames;
  std::size_t max_detections_per_frame;
  std::size_t max_response_bytes;
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

struct ProbeContext {
  explicit ProbeContext(const Request &request) : request(request) {}

  const Request &request;
  std::vector<Frame> frames;
  std::uint64_t processed_frames = 0;
  std::uint64_t eligible_ordinal = 0;
  std::size_t estimated_response_bytes = 128;
  std::mutex error_mutex;
  std::string error;

  void fail(std::string message) {
    std::lock_guard lock(error_mutex);
    if (error.empty()) {
      error = std::move(message);
    }
  }

  [[nodiscard]] std::string error_copy() {
    std::lock_guard lock(error_mutex);
    return error;
  }
};

struct DemuxContext {
  GstElement *queue;
  ProbeContext *probe;
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
  const char *value =
      json_object_get_string(required_member(object, name, json_type_string));
  if (value == nullptr || *value == '\0') {
    fail(std::string("JSON field `") + name + "` must not be empty");
  }
  return value;
}

std::int64_t required_i64(json_object *object, const char *name) {
  return json_object_get_int64(required_member(object, name, json_type_int));
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
         " is not a readable regular file: " + path.string());
  }
}

Sampling parse_sampling(json_object *object) {
  const auto mode = required_string(object, "mode");
  if (mode == "every_frame") {
    return {SamplingMode::EveryFrame, 1};
  }
  if (mode == "every_nth") {
    return {SamplingMode::EveryNth, required_positive_u64(object, "step")};
  }
  if (mode == "maximum_frames") {
    return {SamplingMode::MaximumFrames,
            required_positive_u64(object, "count")};
  }
  fail("unsupported sampling mode `" + mode + "`");
}

Request parse_request(const std::filesystem::path &request_path,
                      const std::filesystem::path &cli_response_path) {
  JsonPtr root(json_object_from_file(request_path.c_str()), &json_object_put);
  if (!root || !json_object_is_type(root.get(), json_type_object)) {
    fail("request JSON is not a valid object");
  }
  if (required_string(root.get(), "schema") != kRequestSchema) {
    fail("unsupported perception runner request schema");
  }
  (void)required_string(root.get(), "task_id");

  Request request;
  request.input_mp4 = required_absolute_path(root.get(), "input_mp4");
  request.input_width = required_positive_u32(root.get(), "input_width");
  request.input_height = required_positive_u32(root.get(), "input_height");
  request.response_json = required_absolute_path(root.get(), "response_json");
  if (request.response_json != cli_response_path) {
    fail("response path argument does not match the typed request");
  }

  auto *pipeline = required_member(root.get(), "pipeline", json_type_object);
  (void)required_string(pipeline, "pipeline_id");
  request.operation = required_string(pipeline, "operation");
  if (request.operation != "object_detection" &&
      request.operation != "object_detection_tracking") {
    fail("the production runner supports only object detection pipelines");
  }
  request.inference_config_path =
      required_absolute_path(pipeline, "deepstream_config_path");

  json_object *tracker = nullptr;
  if (json_object_object_get_ex(pipeline, "tracker", &tracker) &&
      !json_object_is_type(tracker, json_type_null)) {
    if (!json_object_is_type(tracker, json_type_object)) {
      fail("JSON field `tracker` has the wrong type");
    }
    TrackerRequest parsed;
    parsed.config_path = required_absolute_path(tracker, "config_path");
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

  auto *model = required_member(root.get(), "model", json_type_object);
  (void)required_string(model, "model_id");
  if (required_string(model, "format") != "tensor_rt_engine") {
    fail("the production runner accepts TensorRT engine models only");
  }
  request.model_path = required_absolute_path(model, "model_path");

  auto *range =
      required_member(root.get(), "requested_range", json_type_object);
  request.requested_range = {required_i64(range, "start"),
                             required_i64(range, "end")};
  if (request.requested_range.start > request.requested_range.end) {
    fail("requested range is reversed");
  }
  request.decode_start_index = required_i64(root.get(), "decode_start_index");
  if (request.decode_start_index > request.requested_range.start) {
    fail("decode start is after the requested range");
  }
  request.sampling =
      parse_sampling(required_member(root.get(), "sampling", json_type_object));
  request.max_output_frames =
      required_positive_size(root.get(), "max_output_frames");
  request.max_detections_per_frame =
      required_positive_size(root.get(), "max_detections_per_frame");
  request.max_response_bytes =
      required_positive_size(root.get(), "max_response_bytes");
  if (request.sampling.mode == SamplingMode::MaximumFrames &&
      request.sampling.value > request.max_output_frames) {
    fail("maximum_frames count exceeds max_output_frames");
  }

  require_regular_file(request.input_mp4, "input MP4");
  require_regular_file(request.inference_config_path,
                       "DeepStream inference config");
  require_regular_file(request.model_path, "TensorRT engine");
  if (request.tracker) {
    require_regular_file(request.tracker->config_path, "tracker config");
    require_regular_file(std::filesystem::path(std::string(kTrackerLibrary)),
                         "DeepStream tracker library");
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
    const auto index = context.request.decode_start_index + pts;
    if (index < context.request.requested_range.start ||
        index > context.request.requested_range.end) {
      continue;
    }
    ++context.processed_frames;
    if (!should_emit(context)) {
      continue;
    }
    if (context.frames.size() >= context.request.max_output_frames) {
      context.fail("DeepStream output exceeded max_output_frames");
      continue;
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
    if (!context.frames.empty() && context.frames.back().index >= frame.index) {
      context.fail("DeepStream frame indices are not strictly increasing");
      continue;
    }
    context.frames.push_back(std::move(frame));
  }
  return GST_PAD_PROBE_OK;
}

void demux_pad_added(GstElement *, GstPad *source_pad, gpointer user_data) {
  auto &context = *static_cast<DemuxContext *>(user_data);
  GstPad *sink_pad = gst_element_get_static_pad(context.queue, "sink");
  if (sink_pad == nullptr) {
    context.probe->fail("failed to retrieve queue sink pad");
    return;
  }
  if (gst_pad_is_linked(sink_pad)) {
    gst_object_unref(sink_pad);
    return;
  }

  GstCaps *caps = gst_pad_get_current_caps(source_pad);
  if (caps == nullptr) {
    caps = gst_pad_query_caps(source_pad, nullptr);
  }
  const GstStructure *structure = caps == nullptr || gst_caps_is_empty(caps)
                                      ? nullptr
                                      : gst_caps_get_structure(caps, 0);
  const char *media_type =
      structure == nullptr ? nullptr : gst_structure_get_name(structure);
  if (media_type != nullptr && g_str_equal(media_type, "video/x-h264")) {
    if (gst_pad_link(source_pad, sink_pad) != GST_PAD_LINK_OK) {
      context.probe->fail("failed to link MP4 video track to H.264 parser");
    }
  } else if (media_type != nullptr && g_str_has_prefix(media_type, "video/")) {
    context.probe->fail("input MP4 video track is not H.264");
  }
  if (caps != nullptr) {
    gst_caps_unref(caps);
  }
  gst_object_unref(sink_pad);
}

GstElement *make_element(const char *factory, const char *name) {
  GstElement *element = gst_element_factory_make(factory, name);
  if (element == nullptr) {
    fail(std::string("required GStreamer element is unavailable: ") + factory);
  }
  return element;
}

void require_link(bool linked, std::string_view description) {
  if (!linked) {
    fail("failed to link " + std::string(description));
  }
}

void run_pipeline(const Request &request, ProbeContext &probe) {
  GstElement *pipeline = gst_pipeline_new("veoveo-perception");
  if (pipeline == nullptr) {
    fail("failed to create GStreamer pipeline");
  }
  auto pipeline_guard = std::unique_ptr<GstElement, void (*)(GstElement *)>(
      pipeline, [](GstElement *value) {
        gst_element_set_state(value, GST_STATE_NULL);
        gst_object_unref(value);
      });

  auto *source = make_element("filesrc", "source");
  auto *demux = make_element("qtdemux", "demux");
  auto *queue = make_element("queue", "decode-queue");
  auto *parser = make_element("h264parse", "h264-parser");
  auto *decoder = make_element("nvv4l2decoder", "hardware-decoder");
  auto *mux = make_element("nvstreammux", "stream-muxer");
  auto *inference = make_element("nvinfer", "primary-inference");
  GstElement *tracker =
      request.tracker ? make_element("nvtracker", "object-tracker") : nullptr;
  auto *sink = make_element("fakesink", "metadata-sink");

  if (tracker != nullptr) {
    gst_bin_add_many(GST_BIN(pipeline), source, demux, queue, parser, decoder,
                     mux, inference, tracker, sink, nullptr);
  } else {
    gst_bin_add_many(GST_BIN(pipeline), source, demux, queue, parser, decoder,
                     mux, inference, sink, nullptr);
  }

  g_object_set(source, "location", request.input_mp4.c_str(), nullptr);
  g_object_set(mux, "batch-size", 1U, "width", request.input_width, "height",
               request.input_height, "live-source", FALSE,
               "batched-push-timeout", 40000, nullptr);
  g_object_set(inference, "config-file-path",
               request.inference_config_path.c_str(), "model-engine-file",
               request.model_path.c_str(), "batch-size", 1U, "process-mode", 1U,
               "interval", 0U, nullptr);
  if (tracker != nullptr) {
    g_object_set(tracker, "ll-lib-file", kTrackerLibrary.data(),
                 "ll-config-file", request.tracker->config_path.c_str(),
                 "tracker-width", request.tracker->width, "tracker-height",
                 request.tracker->height, nullptr);
  }
  g_object_set(sink, "sync", FALSE, "async", FALSE, nullptr);

  require_link(gst_element_link(source, demux), "file source to MP4 demuxer");
  require_link(gst_element_link_many(queue, parser, decoder, nullptr),
               "H.264 decode chain");
  GstPad *decoder_source = gst_element_get_static_pad(decoder, "src");
  GstPad *mux_sink = gst_element_request_pad_simple(mux, "sink_0");
  if (decoder_source == nullptr || mux_sink == nullptr ||
      gst_pad_link(decoder_source, mux_sink) != GST_PAD_LINK_OK) {
    if (decoder_source != nullptr) {
      gst_object_unref(decoder_source);
    }
    if (mux_sink != nullptr) {
      gst_object_unref(mux_sink);
    }
    fail("failed to link hardware decoder to nvstreammux");
  }
  gst_object_unref(decoder_source);
  gst_object_unref(mux_sink);
  require_link(gst_element_link(mux, inference), "nvstreammux to nvinfer");
  if (tracker != nullptr) {
    require_link(gst_element_link_many(inference, tracker, sink, nullptr),
                 "inference, tracker, and metadata sink");
  } else {
    require_link(gst_element_link(inference, sink),
                 "inference to metadata sink");
  }

  DemuxContext demux_context{queue, &probe};
  g_signal_connect(demux, "pad-added", G_CALLBACK(demux_pad_added),
                   &demux_context);
  GstPad *sink_pad = gst_element_get_static_pad(sink, "sink");
  if (sink_pad == nullptr) {
    fail("failed to retrieve metadata sink pad");
  }
  gst_pad_add_probe(sink_pad, GST_PAD_PROBE_TYPE_BUFFER, inference_probe,
                    &probe, nullptr);
  gst_object_unref(sink_pad);

  if (gst_element_set_state(pipeline, GST_STATE_PLAYING) ==
      GST_STATE_CHANGE_FAILURE) {
    fail("DeepStream pipeline refused the PLAYING state");
  }
  GstBus *bus = gst_element_get_bus(pipeline);
  GstMessage *message = gst_bus_timed_pop_filtered(
      bus, GST_CLOCK_TIME_NONE,
      static_cast<GstMessageType>(GST_MESSAGE_ERROR | GST_MESSAGE_EOS));
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
  if (probe.processed_frames == 0) {
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
      json_object_object_add(detection_json, "class_id",
                             json_object_new_uint64(detection.class_id));
      json_object_object_add(detection_json, "label",
                             json_object_new_string(detection.label.c_str()));
      add_optional_double(detection_json, "confidence", detection.confidence);
      add_optional_double(detection_json, "tracker_confidence",
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
        json_object_object_add(detection_json, "track_id",
                               json_object_new_uint64(*detection.track_id));
      }
      json_object_array_add(detections, detection_json);
    }
    json_object_object_add(frame_json, "detections", detections);
    json_object_array_add(frames, frame_json);
  }
  json_object_object_add(root.get(), "frames", frames);
  json_object_object_add(root.get(), "processed_frames",
                         json_object_new_uint64(probe.processed_frames));
  json_object_object_add(root.get(), "elapsed_ms",
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

std::pair<std::filesystem::path, std::filesystem::path>
parse_arguments(int argc, char **argv) {
  std::optional<std::filesystem::path> request;
  std::optional<std::filesystem::path> response;
  for (int index = 1; index < argc; ++index) {
    const std::string_view argument(argv[index]);
    if ((argument == "--request-json" || argument == "--response-json") &&
        index + 1 >= argc) {
      fail(std::string(argument) + " requires a path");
    }
    if (argument == "--request-json") {
      request = std::filesystem::path(argv[++index]);
    } else if (argument == "--response-json") {
      response = std::filesystem::path(argv[++index]);
    } else {
      fail("unknown runner argument `" + std::string(argument) + "`");
    }
  }
  if (!request || !response || !request->is_absolute() ||
      !response->is_absolute()) {
    fail("--request-json and --response-json require absolute paths");
  }
  return {*request, *response};
}

} // namespace

int main(int argc, char **argv) {
  try {
    const auto [request_path, response_path] = parse_arguments(argc, argv);
    redirect_native_stdout_to_stderr();
    gst_init(nullptr, nullptr);
    const auto request = parse_request(request_path, response_path);
    ProbeContext probe(request);
    const auto started = std::chrono::steady_clock::now();
    run_pipeline(request, probe);
    const auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
                             std::chrono::steady_clock::now() - started)
                             .count();
    write_response(request, probe, static_cast<std::uint64_t>(elapsed));
    return 0;
  } catch (const std::exception &error) {
    std::fprintf(stderr, "perception-deepstream-runner: %s\n", error.what());
    return 1;
  }
}

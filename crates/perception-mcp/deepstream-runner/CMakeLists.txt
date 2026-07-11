cmake_minimum_required(VERSION 3.22)
project(perception_deepstream_runner LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

find_package(PkgConfig REQUIRED)
pkg_check_modules(GSTREAMER REQUIRED IMPORTED_TARGET gstreamer-1.0)
pkg_check_modules(JSON_C REQUIRED IMPORTED_TARGET json-c)

set(DEEPSTREAM_ROOT "/opt/nvidia/deepstream/deepstream" CACHE PATH "DeepStream installation")
find_library(
  NVDS_META_LIBRARY
  NAMES nvds_meta
  PATHS "${DEEPSTREAM_ROOT}/lib"
  NO_DEFAULT_PATH
  REQUIRED
)
find_library(
  NVDS_GST_META_LIBRARY
  NAMES nvdsgst_meta
  PATHS "${DEEPSTREAM_ROOT}/lib"
  NO_DEFAULT_PATH
  REQUIRED
)

add_executable(perception-deepstream-runner src/main.cpp)
target_include_directories(
  perception-deepstream-runner
  PRIVATE "${DEEPSTREAM_ROOT}/sources/includes"
)
target_link_libraries(
  perception-deepstream-runner
  PRIVATE
    PkgConfig::GSTREAMER
    PkgConfig::JSON_C
    "${NVDS_META_LIBRARY}"
    "${NVDS_GST_META_LIBRARY}"
)
target_compile_options(perception-deepstream-runner PRIVATE -Wall -Wextra -Wpedantic)

install(TARGETS perception-deepstream-runner RUNTIME DESTINATION bin)

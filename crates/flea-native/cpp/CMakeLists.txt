cmake_minimum_required(VERSION 3.20)
project(flea_native LANGUAGES CXX)

set(CMAKE_AUTOMOC ON)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 REQUIRED COMPONENTS Quick Qml)

set(RUST_LIB "${CMAKE_CURRENT_SOURCE_DIR}/../target/release/libflea_native.a")

qt_add_qml_module(fleanative
    URI FleaNative
    VERSION 1.0
    OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/FleaNative"
    SOURCES
        flea_dir_model.cpp
        flea_dir_model.h
)

target_link_libraries(fleanative PRIVATE
    ${RUST_LIB}
    dl
    pthread
)

# build.sh copies the plugin and libfleanative.so side by side into ui/modules/FleaNative, so both
# look beside themselves (and one up, for the build tree's layout) rather than in this build directory.
set_target_properties(fleanative fleanativeplugin PROPERTIES
    BUILD_RPATH "$ORIGIN;$ORIGIN/.."
    INSTALL_RPATH "$ORIGIN"
)

# CMake toolchain file used for the Windows build of SDL (see config.toml).
# SDL's CMake never picks the C runtime itself: it takes CMAKE_MSVC_RUNTIME_LIBRARY, which
# defaults to the DLL runtime (/MD). SDL's own release script sets it to MultiThreaded (/MT).
set(CMAKE_MSVC_RUNTIME_LIBRARY MultiThreaded)

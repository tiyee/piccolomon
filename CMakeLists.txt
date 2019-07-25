cmake_minimum_required(VERSION 2.8)
project(piccolomon)
set(CMAKE_CXX_STANDARD 11)
set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -std=c++11 ")
set(CMAKE_CXX_FLAGS " ${CMAKE_CXX_FLAGS} -stdlib=libc++")
SET(EXECUTABLE_OUTPUT_PATH ${PROJECT_SOURCE_DIR}/bin)
SET(LIBRARY_OUTPUT_PATH ${PROJECT_SOURCE_DIR}/lib)

SET(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS}  -m64 -O2")




MESSAGE("++++++++++++++Start Build+++++++++++++++++")



add_executable(${PROJECT_NAME} src/main.cpp )
add_library (H src/h.cpp)
target_link_libraries(${PROJECT_NAME} H)






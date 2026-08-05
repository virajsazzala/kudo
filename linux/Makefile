CC      := gcc
TARGET  := kudo

# Directories
SRC_DIR      := src
PLATFORM_DIR := platform/linux
INC_DIR      := include
BUILD_DIR    := build

# Collect all .c sources
SRCS := $(shell find $(SRC_DIR) $(PLATFORM_DIR) -name '*.c')
OBJS := $(patsubst %.c, $(BUILD_DIR)/%.o, $(SRCS))

# Flags
CFLAGS_COMMON := -Wall -Wextra -Wpedantic \
                 -I$(INC_DIR) \
                 -I$(PLATFORM_DIR) \
                 $(shell pkg-config --cflags dbus-1)

CFLAGS_DEBUG   := $(CFLAGS_COMMON) -g -O0 -DDEBUG
CFLAGS_RELEASE := $(CFLAGS_COMMON) -O2 -DNDEBUG

LDFLAGS := $(shell pkg-config --libs dbus-1)

# Default to debug
CFLAGS ?= $(CFLAGS_DEBUG)

.PHONY: all debug release clean

all: debug

debug: CFLAGS = $(CFLAGS_DEBUG)
debug: $(TARGET)

release: CFLAGS = $(CFLAGS_RELEASE)
release: $(TARGET)

$(TARGET): $(OBJS)
	$(CC) $(OBJS) $(LDFLAGS) -o $@

$(BUILD_DIR)/%.o: %.c
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -rf $(BUILD_DIR) $(TARGET)
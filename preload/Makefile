CC = gcc
CFLAGS = -Wall -fPIC -shared
LIBS = -lX11 -ldl

TARGET = lib0fbuf_preload.so
SRC = 0fbuf_preload.c

all: $(TARGET)

$(TARGET): $(SRC)
	$(CC) $(CFLAGS) -o $(TARGET) $(SRC) $(LIBS)

clean:
	rm -f $(TARGET)

install: $(TARGET)
	cp $(TARGET) ~/.local/lib/$(TARGET)

.PHONY: all clean install

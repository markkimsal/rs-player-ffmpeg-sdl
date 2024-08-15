RS Player
===
Example project using rusty\_ffmpeg to create desktop player.


Rusty ffmpeg generates rust bindings for all of the libs in the ffmpeg project.  Rust ffmpeg will link to 
statically compiled \*.a files.  But, if you have a compiled version of ffmpeg libraries that is the same
version that rusty ffmpeg uses to generate bindings, you can pre-load the shared objects and the linking
will work. (at least on x86 desktops).

The goal is to have a program that can be used to explore all the ffmpeg libraries, mainly sws\_scale, colorspace
conversion, and filters.

Goals:
 * Example app to understand Rust FFI bindings and the behavior of raw pointers, unsafe blocks, and managing memory when integrating with C libraries.
 * Understanding how to link libraries via FFI for other platforms (like Android and iOS)

ffmpeg Prebuilt
===
Use ffmpeg-kit project to build ffmpeg for your desktop platform.

For clickable "run" functionality in vscode, update your `.vscode/settings.json` and add
```json
    "rust-analyzer.runnableEnv": {
        "LD_LIBRARY_PATH" : "/path/to/ffmpeg-kit/prebuilt/linux-x86_64/ffmpeg/lib",
        "FFMPEG_PKG_CONFIG_PATH" : "/path/to/ffmpeg-kit/prebuilt/linux-x86_64/ffmpeg/lib/pkgconfig"
    }
```

Generating a test video
===
```
./scripts/make-test-vid.sh
```
Will overwrite `test_vid.mp4` in the current directory.

Setting up VSCode
===
You can create `cargo` type tasks like this:
```json
        {
            "type": "cargo",
            "command": "run",
            "args": [
                "--package",
                "sdl",
                "--bin",
                "sdl",
                "test_vid.mp4"
            ],
            "env": {
                "RUST_BACKTRACE": "1",
                "FFMPEG_PKG_CONFIG_PATH": "ffmpeg-kit/prebuilt/linux-x86_64/ffmpeg/lib/pkgconfig",
                "LD_LIBRARY_PATH": "ffmpeg-kit/prebuilt/linux-x86_64/ffmpeg/lib"
            },
            "problemMatcher": [
                "$rustc"
            ],
            "group": "build",
            "label": "rust: run sdl"
        },
        {
            "type": "cargo",
            "command": "build",
            "args": [
                "--package",
                "sdl"
            ],
            "env": {
                "RUST_BACKTRACE": "1",
                "FFMPEG_PKG_CONFIG_PATH": "ffmpeg-kit/prebuilt/linux-x86_64/ffmpeg/lib/pkgconfig",
                "LD_LIBRARY_PATH": "ffmpeg-kit/prebuilt/linux-x86_64/ffmpeg/lib"
            },
            "problemMatcher": [
                "$rustc"
            ],
            "group": "build",
            "label": "rust: build sdl"
        }
```

This enables quick tasks from `ctrl-d`.  You can either always set `FFMPEG_PKG_CONFIG_PATH` and `LD_LIBRARY_PATH` for every task
or set them once in your shell before launching vscode.

You can also put these envs into your `.cargo/config.toml` file under the `[env]` section.

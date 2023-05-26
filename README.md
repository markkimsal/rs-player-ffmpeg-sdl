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

Passing movie file to the application
===
In order to play a movie like `foo.mp4`, you must edit your `.vscode/settings.json` and add
```json
	"rust-analyzer.runnables.extraArgs": [
		"foo.mp4"
	],
```


Passing movie file to the application in debug
===
There doesn't seem to be anyway to pass command line arguments to the rust-analyzer when debugging.
Therefore, you should put any movie file you want to debug in the root foler as `foo.mp4` as this
is the fallback value for when no command line parameters are passed in.

log
===
I was able to get recording working following some ffmpeg samples.  Taking a decoded frame and passing it right back to the encoder works.

But, I really want to record drawings on the video.  Perhaps a blend of 2 textures from SDL.  So, I tried to access the SDL surface memory, and
that's where I got really stuck.  The SDL pixel pointer is opaque c\_void.  No documentation on how the pixels are put in there. (there is some 
documentation, but I can't really make sense of it)  The hex viewer in vscode kind of sucks. And so, slicing from raw pointer does really
give good information as to what is going on. I just get a completely green video.

So, back to ffmpeg samples.  There's `encode_video.c`, which generates a pattern of colors.  I copied that code and attached the pixel buffer's
directly to frame-\>data[0], 1 and 2.  This produces a green video where the top 2 scan lines have some blocky, mis-aligned motion.  I don't know
if the generated data is square or not, or if that matters. (encode\_video.c produces a square video, but adjusting the w/h gives a 16:9 video)

I'm not certain if I'm accessing the memory pointers correctly in Rust, so I'm going to translate the `encode\_video.c` sample directly into 
Rust in a new project.


rewrite encode_video.c
===
Okay, that solved the problem.  I had Codeium tab complete a lot of lines and it dealt with the pointers differnetly than I did.
Essentially:

```rs
        dest_frame.data[0] = ::std::ptr::addr_of_mut!(ybuff) as *mut _;
        dest_frame.data[1] = ::std::ptr::addr_of_mut!(cbbuff) as *mut _;
        dest_frame.data[2] = ::std::ptr::addr_of_mut!(crbuff) as *mut _;

        // but it should be

        dest_frame.data[1] = ::std::ptr::addr_of_mut!(*ybuff) as *mut _;
        dest_frame.data[1] = ::std::ptr::addr_of_mut!(*cbbuff) as *mut _;
        dest_frame.data[2] = ::std::ptr::addr_of_mut!(*crbuff) as *mut _;
```
Or, as Codeium wrote it

```rs
        dest_frame.data[0] = ybuff.as_mut_ptr() as *mut _;
        dest_frame.data[1] = cbbuff.as_mut_ptr() as *mut _;
        dest_frame.data[2] = crbuff.as_mut_ptr() as *mut _;
```

In the first one, I was sending the address of the rust variable, not the underlying data.

Also, a few other things about casting data to `u8`s that I did not do.

I didn't know `0u8` was a data type:

```rs
        let mut ybuff = vec![0u8; (frame.linesize[0] as i32 * frame.height) as usize];
```

That took so much longer than I wanted it to. That was probably a 5 hour road block.  I should have re-coded the example
program sooner.  I knew it was a pointer problem right away, but I got distracted trying to understand the layout of the SDL
YUV info.  After copying the test-pattern generating code, that also didn't create a valid video, but the compiled C program 
did.  So, without Codeium, I'm sure I would have repeated my earlier mistake of `addr_of_mut` instead of `as_mut_ptr()`.

I think I should make an ffmpeg-lings.

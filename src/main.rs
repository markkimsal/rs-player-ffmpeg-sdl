use rsplayer::movie_state::MovieState;
use rsplayer::app::{open_movie, play_movie, drop_movie_state};

mod movie_state;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // you can't pass cli arguments to debug with rust-analyzer
    let default_file = String::from("foo.mp4");
    let mut video_state = MovieState::new();
    unsafe {
        let filepath: std::ffi::CString = std::ffi::CString::new(default_file).unwrap();
        let filepath: *const std::os::raw::c_char = filepath.as_ptr();
        // open_movie(args.get(1).unwrap_or(&default_file.as_ptr()), &mut video_state);
        open_movie(filepath, &mut video_state);
    }
    // let (_, format_context, codec_context) = open_input(args.get(1).unwrap_or(&default_file));
    // open_window(format_context, codec_context);
    unsafe {play_movie(&mut video_state); }
    unsafe {drop_movie_state(&mut video_state); }
}

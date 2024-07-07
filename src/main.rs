use rsplayer::movie_state::MovieState;
use rsplayer::app::{open_movie, play_movie};

mod movie_state;

fn main() {

    let mut clog = colog::default_builder();
    clog.filter(None, log::LevelFilter::Info);
    clog.init();

    let args: Vec<String> = std::env::args().collect();
    // you can't pass cli arguments to debug with rust-analyzer
    let default_file = String::from("foo.mp4");
    let mut video_state = MovieState::new();
    unsafe {
        let filepath: std::ffi::CString = std::ffi::CString::new(args.get(1).unwrap_or(&default_file).as_str()).unwrap();
        open_movie(filepath.as_ptr(), &mut video_state);
    }
    // open_window(format_context, codec_context);
    unsafe {play_movie(&mut video_state); }
}

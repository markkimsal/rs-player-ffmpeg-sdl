use rsplayer::movie_state::MovieState;
use rsplayer::app::{open_movie, play_movie, RsPlayerContext};

use rsplayer::platform;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // you can't pass cli arguments to debug with rust-analyzer
    let mut video_state = MovieState::new();
    let mut _player_context = RsPlayerContext {
        init:  platform::sdl::init_subsystem as _,
        frame: platform::sdl::event_loop,
        subsystem_ctx: None,
    };

    unsafe {
        let default_file = String::from("foo.mp4");
        let filepath: std::ffi::CString = std::ffi::CString::new(args.get(1).unwrap_or(&default_file).as_str()).unwrap();
        open_movie(filepath.as_ptr(), &mut video_state);
        play_movie(&mut video_state);
    }
}

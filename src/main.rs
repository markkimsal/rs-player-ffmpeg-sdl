use rsplayer::movie_state::MovieState;
use rsplayer::app::{open_movie, play_movie};

mod movie_state;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // you can't pass cli arguments to debug with rust-analyzer
    let default_file = String::from("foo.mp4");
    let mut video_state = MovieState::new();
    unsafe {
        open_movie(args.get(1).unwrap_or(&default_file), &mut video_state);
    }
    // let (_, format_context, codec_context) = open_input(args.get(1).unwrap_or(&default_file));
    // open_window(format_context, codec_context);
    play_movie(video_state);
}

#![allow(unused_variables, dead_code, unused)]
use crate::movie_state::MovieState;

pub struct AnalyzerContext {
    pub movie_list: Vec<MovieState>,
}

impl AnalyzerContext {
    pub fn new() -> AnalyzerContext {
        AnalyzerContext {
            movie_list: vec![],
        }
    }
}

impl AnalyzerContext {
    pub fn add_movie_state(&mut self, movie: MovieState) {
        self.movie_list.push(movie);
    }
}

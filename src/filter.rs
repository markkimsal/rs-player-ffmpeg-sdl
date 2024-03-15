
#![allow(unused_variables, non_camel_case_types)]
use std::ffi::{CString, CStr};
use rusty_ffmpeg::ffi::{self};


/*
enum pix_fmts {
    AVPixelFormat_AV_PIX_FMT_YUV420P,
    AVPixelFormat_AV_PIX_FMT_NONE
}
*/
pub struct RotateFilter {
    pub filter_graph: *mut ffi::AVFilterGraph,
    pub buffersink_ctx: *mut ffi::AVFilterContext,
    pub buffersrc_ctx: *mut ffi::AVFilterContext
}

//pub fn init_filter(avblock* block, char* filters_descr) -> i32
pub fn init_filter(
    rotation: i32,
    filter_graph: &mut *mut ffi::AVFilterGraph,
    buffersink_ctx: &mut *mut ffi::AVFilterContext,
    buffersrc_ctx: &mut *mut ffi::AVFilterContext,
    wh: (i32, i32),
    format: i32,
) -> i32 {
	let ret: i32 = 0;
    let buffer_src_name = CString::new("buffer").unwrap();
    let buffer_sink_name = CString::new("buffersink").unwrap();
	let buffer_src: *const ffi::AVFilter = unsafe { ffi::avfilter_get_by_name(buffer_src_name.as_ptr()) };
	let buffer_sink: *const ffi::AVFilter = unsafe { ffi::avfilter_get_by_name(buffer_sink_name.as_ptr()) };
	let mut outputs: *mut ffi::AVFilterInOut = unsafe { ffi::avfilter_inout_alloc() };
	let mut inputs: *mut ffi::AVFilterInOut = unsafe { ffi::avfilter_inout_alloc() };

	let time_base: ffi::AVRational  = ffi::AVRational{num: 1, den: 240};

	unsafe {
        if !buffersink_ctx.is_null() {ffi::avfilter_free(*buffersink_ctx);}
        if !buffersrc_ctx.is_null() {ffi::avfilter_free(*buffersrc_ctx);}
        ffi::avfilter_graph_free(filter_graph) ;
        *filter_graph = ffi::avfilter_graph_alloc();

        if outputs.is_null() || inputs.is_null() || filter_graph.is_null() {
            ffi::avfilter_graph_config(*filter_graph as *mut _, std::ptr::null_mut());
            ffi::avfilter_inout_free(&mut inputs  as *mut _);
            ffi::avfilter_inout_free(&mut outputs  as *mut _);
            return ffi::AVERROR(ffi::ENOMEM);
        }
    }

    let width = wh.0;
    let height = wh.1;
    // let width = 450;
    // let height = 800;
	// assume source is AV_PIX_FMT_YUV420P;
	/* buffer video source: the decoded frames from the decoder will be inserted here. */
    let args = format!(
        "video_size={}x{}:pix_fmt={}:time_base={}/{}:pixel_aspect={}/{}",
        width, height,
        // ffi::AVPixelFormat_AV_PIX_FMT_ARGB,
        format as ffi::AVPixelFormat,
        time_base.num, time_base.den, 1, 1
    );
    let args = &CString::new(args).unwrap();
    println!(
        "video_size={}x{}:pix_fmt={}:time_base={}/{}:pixel_aspect={}/{}",
        width, height,
        // ffi::AVPixelFormat_AV_PIX_FMT_ARGB,
        format as ffi::AVPixelFormat,
        time_base.num, time_base.den, 1, 1
    );

    unsafe {
        let in_buff = CString::new("in").unwrap();
        let ret = ffi::avfilter_graph_create_filter(
            buffersrc_ctx,
            buffer_src,
            (&in_buff as &CStr).as_ptr(),
            // std::ptr::null(),
            args.as_ptr() as *const _,
            std::ptr::null_mut(),
            *filter_graph
        );
        if ret < 0 {
            ffi::avfilter_inout_free(&mut inputs  as *mut _);
            ffi::avfilter_inout_free(&mut outputs as *mut _);
            return ret;
        }
        // The buffer source output must be connected to the input pad of
        // the first filter described by filters_descr; since the first
        // filter input label is not specified, it is set to "in" by
        // default.
        (*outputs).name       = ffi::av_strdup(in_buff.as_ptr());
        (*outputs).filter_ctx = *buffersrc_ctx;
        (*outputs).pad_idx    = 0;
        (*outputs).next       = std::ptr::null_mut();

    }


    unsafe {
        let out_buff = CString::new("out").unwrap();
        /* buffer video sink: to terminate the filter chain. */
        let ret = ffi::avfilter_graph_create_filter(
            buffersink_ctx as *mut _,
            buffer_sink,
            out_buff.as_ptr(),
            std::ptr::null(),
            // args.as_ptr() as *const _,
            std::ptr::null_mut(),
            *filter_graph
        );
        if ret < 0 {
            ffi::avfilter_inout_free(&mut inputs  as *mut _);
            ffi::avfilter_inout_free(&mut outputs as *mut _);
            return ret;
        }

        // The buffer sink input must be connected to the output pad of
        // the last filter described by filters_descr; since the last
        // filter output label is not specified, it is set to "out" by
        // default.
        (*inputs).name       = ffi::av_strdup(out_buff.as_ptr());
        (*inputs).filter_ctx = *buffersink_ctx;
        (*inputs).pad_idx    = 0;
        (*inputs).next       = std::ptr::null_mut();
    }

// unsafe {
//     let key = CString::new("pix_fmts").unwrap();
//     let result = ffi::av_opt_set_pixel_fmt(*buffersink_ctx as _, key.as_ptr(), ffi::AVPixelFormat_AV_PIX_FMT_YUV420P, AV_OPT_SEARCH_CHILDREN as i32);
//     println!("result {}", result);
// }
// 	ret = av_opt_set_int_list(block->buffersink_ctx, "pix_fmts", pix_fmts,
// 			AV_PIX_FMT_NONE, AV_OPT_SEARCH_CHILDREN);
// 	if (ret < 0) {
// 		avfilter_inout_free(&inputs);
// 		avfilter_inout_free(&outputs);
// 		return ret;
// 	}

    //
    // Set the endpoints for the filter graph. The filter_graph will
    // be linked to the graph described by filters_descr.
    //
    unsafe {
        let transpose = match rotation {
            90 => "transpose=1",
            270 => "transpose=2",
            -90 => "transpose=3",
            _  => "tpad=0",
        };
        let filter_desc = CString::new(transpose).unwrap();
        let ret = ffi::avfilter_graph_parse_ptr(
            *filter_graph,
            filter_desc.as_ptr(),
            &mut inputs as *mut _,
            &mut outputs as *mut _,
            std::ptr::null_mut()
        );
        if ret >= 0 {
            ffi::avfilter_graph_config(*filter_graph as *mut _, std::ptr::null_mut());
        }
		ffi::avfilter_inout_free(&mut inputs  as *mut _);
		ffi::avfilter_inout_free(&mut outputs  as *mut _);
    }
    return ret;
}

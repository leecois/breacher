//! WebAssembly surface of the core: every result is a little-endian `u32` length followed by that many bytes of JSON.
#![allow(clippy::missing_safety_doc)]
use breach_core::api::{self, Response};
use breach_core::follow::Followed;
use breach_core::glyph::{Gate, Templates};
use breach_core::image::{Frame, PixelFormat};
use breach_core::plan::{plan, rule_check, PlanOptions};
use breach_core::reader::{buffer_codes, ReadOptions, Reading};
use std::alloc::{alloc, dealloc, Layout};
use std::sync::OnceLock;

static TEMPLATES: OnceLock<Templates> = OnceLock::new();

#[no_mangle]
pub extern "C" fn bp_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::<u8>::dangling().as_ptr();
    }
    unsafe { alloc(Layout::from_size_align_unchecked(len, 1)) }
}

/// Safety: `ptr` must come from `bp_alloc(len)`, or be an `out` pointer with `len = 4 + length`.
#[no_mangle]
pub unsafe extern "C" fn bp_free(ptr: *mut u8, len: usize) {
    if len != 0 && !ptr.is_null() {
        unsafe { dealloc(ptr, Layout::from_size_align_unchecked(len, 1)) };
    }
}

fn out(json: String) -> *mut u8 {
    let bytes = json.into_bytes();
    let p = bp_alloc(bytes.len() + 4);
    unsafe {
        std::ptr::copy_nonoverlapping((bytes.len() as u32).to_le_bytes().as_ptr(), p, 4);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.add(4), bytes.len());
    }
    p
}

/// Request options: the plan options, plus the buffer size to assume when the panel is unreadable.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct Request {
    #[serde(flatten)]
    plan: PlanOptions,
    buffer_fallback: Option<usize>,
}

unsafe fn options(ptr: *const u8, len: usize) -> Result<Request, String> {
    if len == 0 {
        return Ok(Request::default());
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    serde_json::from_slice(bytes).map_err(|e| format!("bad options: {e}"))
}

fn error(message: String) -> *mut u8 {
    out(serde_json::json!({ "status": "error", "message": message }).to_string())
}

/// Safety: `rgba` must point at `width * height * 4` readable bytes and `opts` at `opts_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn bp_read(
    rgba: *const u8,
    width: usize,
    height: usize,
    opts: *const u8,
    opts_len: usize,
) -> *mut u8 {
    let req = match unsafe { options(opts, opts_len) } {
        Ok(o) => o,
        Err(e) => return error(e),
    };
    let data = unsafe { std::slice::from_raw_parts(rgba, width * height * 4) };
    let frame = Frame::new(width, height, PixelFormat::Rgba8, data);
    let templates = TEMPLATES.get_or_init(Templates::builtin);
    let read_opt = ReadOptions { buffer_fallback: req.buffer_fallback, ..ReadOptions::default() };
    let resp = api::read_and_plan(&frame, templates, &read_opt, &req.plan);
    out(serde_json::to_string(&resp)
        .unwrap_or_else(|e| format!("{{\"status\":\"error\",\"message\":\"{e}\"}}")))
}

/// Re-plans an earlier reading. Safety: both pointers must be readable for their lengths.
#[no_mangle]
pub unsafe extern "C" fn bp_plan(
    reading: *const u8,
    reading_len: usize,
    opts: *const u8,
    opts_len: usize,
) -> *mut u8 {
    let plan_opt = match unsafe { options(opts, opts_len) } {
        Ok(o) => o.plan,
        Err(e) => return error(e),
    };
    let bytes = unsafe { std::slice::from_raw_parts(reading, reading_len) };
    let reading: Reading = match serde_json::from_slice(bytes) {
        Ok(r) => r,
        Err(e) => return error(format!("bad reading: {e}")),
    };
    let p = plan(&reading, &plan_opt);
    out(serde_json::to_string(&Response::Ok { reading, plan: p }).unwrap())
}

/// Finds the rule check an earlier reading offers. Safety: both pointers must be readable for their lengths.
#[no_mangle]
pub unsafe extern "C" fn bp_check(
    reading: *const u8,
    reading_len: usize,
    opts: *const u8,
    opts_len: usize,
) -> *mut u8 {
    let plan_opt = match unsafe { options(opts, opts_len) } {
        Ok(o) => o.plan,
        Err(e) => return error(e),
    };
    let bytes = unsafe { std::slice::from_raw_parts(reading, reading_len) };
    let reading: Reading = match serde_json::from_slice(bytes) {
        Ok(r) => r,
        Err(e) => return error(format!("bad reading: {e}")),
    };
    out(match rule_check(&reading, &plan_opt) {
        Some(check) => serde_json::json!({ "status": "ok", "check": check }).to_string(),
        None => serde_json::json!({ "status": "none" }).to_string(),
    })
}

/// A breach being followed, as the page keeps it between looks, with the choices its route was planned under.
#[derive(serde::Deserialize)]
struct Following {
    #[serde(flatten)]
    followed: Followed,
    #[serde(default)]
    priority: breach_core::plan::Priority,
    #[serde(default)]
    matcher: Option<breach_core::solver::Matcher>,
}

/// Looks at a later frame of a screen read earlier and returns the breach as it now stands. Safety: `rgba` must point at `width * height * 4` readable bytes and `state` at `state_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn bp_follow(
    rgba: *const u8,
    width: usize,
    height: usize,
    state: *const u8,
    state_len: usize,
) -> *mut u8 {
    let bytes = unsafe { std::slice::from_raw_parts(state, state_len) };
    let Following { mut followed, priority, matcher } = match serde_json::from_slice(bytes) {
        Ok(f) => f,
        Err(e) => return error(format!("bad state: {e}")),
    };
    let data = unsafe { std::slice::from_raw_parts(rgba, width * height * 4) };
    let frame = Frame::new(width, height, PixelFormat::Rgba8, data);
    // a route the page planned afresh knows nothing of the picks already made
    let caught_up = if !followed.played.is_empty() && followed.route.done != followed.played.len() {
        followed.settle(priority, matcher)
    } else {
        None
    };
    let looked = followed.look(&frame, priority, matcher);
    out(serde_json::json!({
        "status": "ok",
        "sight": looked.sight,
        "step": looked.step.or(caught_up),
        "settled": looked.settled,
        "route": followed.route,
        "played": followed.played,
        "fates": followed.fates,
        "check": followed.check,
    })
    .to_string())
}

/// Reads what ended up in the buffer from a frame of the screen that follows a breach, and holds it against the rules still in play. Safety: as for `bp_follow`.
#[no_mangle]
pub unsafe extern "C" fn bp_learn(
    rgba: *const u8,
    width: usize,
    height: usize,
    state: *const u8,
    state_len: usize,
) -> *mut u8 {
    let bytes = unsafe { std::slice::from_raw_parts(state, state_len) };
    let Following { followed, matcher, .. } = match serde_json::from_slice(bytes) {
        Ok(f) => f,
        Err(e) => return error(format!("bad state: {e}")),
    };
    let data = unsafe { std::slice::from_raw_parts(rgba, width * height * 4) };
    let frame = Frame::new(width, height, PixelFormat::Rgba8, data);
    let templates = TEMPLATES.get_or_init(Templates::builtin);
    let codes = buffer_codes(&frame, &followed.reading, templates, &Gate::default());
    let learnt = codes.as_ref().and_then(|codes| followed.learn(codes, matcher));
    out(serde_json::json!({ "status": "ok", "codes": codes, "learnt": learnt }).to_string())
}

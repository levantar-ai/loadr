//! Native `__loadr_*` functions registered on every VU context.
//!
//! The JS prelude (`prelude.js`) wraps these into the script-facing API
//! (`http`, `check`, `sleep`, `group`, metric classes, `crypto`, `encoding`,
//! `__ENV`, `open`, `console`, `session`).

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine as _;
use hmac::{KeyInit as _, Mac as _, SimpleHmac};
use loadr_core::metrics::MetricKind;
use loadr_core::script::{HostHttpRequest, HostHttpResponse, ScriptHost, ScriptLogLevel};
use md5::Md5;
use rand::Rng as _;
use rquickjs::function::{Func, Opt};
use rquickjs::{ArrayBuffer, Coerced, Ctx, Exception, FromJs, Object, TypedArray, Value};
use sha1::Sha1;
use sha2::{Digest as _, Sha256, Sha384, Sha512};

use crate::host_bridge::HostCell;

/// Maximum byte count accepted by `crypto.randomBytes`.
const MAX_RANDOM_BYTES: usize = 1024 * 1024;

/// Run a host operation, converting a missing host into a JS exception.
fn host_call<'js, R>(
    ctx: &Ctx<'js>,
    cell: &HostCell,
    f: impl FnOnce(&mut dyn ScriptHost) -> R,
) -> rquickjs::Result<R> {
    cell.with_host(f).map_err(|_| {
        Exception::throw_message(
            ctx,
            "this API is only available while a test is executing (no host attached; \
             move the call out of module top-level code)",
        )
    })
}

/// Register every native function on the context's global object.
pub fn register<'js>(ctx: &Ctx<'js>, host: &HostCell) -> rquickjs::Result<()> {
    let globals = ctx.globals();

    // --- host-backed primitives -------------------------------------------
    let h = host.clone();
    globals.set(
        "__loadr_sleep",
        Func::from(move |cx: Ctx<'js>, seconds: f64| -> rquickjs::Result<()> {
            host_call(&cx, &h, |host| host.sleep(seconds))
        }),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_check",
        Func::from(
            move |cx: Ctx<'js>, name: String, pass: bool| -> rquickjs::Result<()> {
                host_call(&cx, &h, |host| host.check(&name, pass))
            },
        ),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_group_push",
        Func::from(move |cx: Ctx<'js>, name: String| -> rquickjs::Result<()> {
            host_call(&cx, &h, |host| host.group_push(&name))
        }),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_group_pop",
        Func::from(move |cx: Ctx<'js>| -> rquickjs::Result<()> {
            host_call(&cx, &h, |host| host.group_pop())
        }),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_metric_add",
        Func::from(
            move |cx: Ctx<'js>,
                  name: String,
                  kind: String,
                  value: f64,
                  tags: Opt<Value<'js>>|
                  -> rquickjs::Result<()> {
                let kind = parse_metric_kind(&cx, &kind)?;
                let tags = match opt_defined::<Object>(&cx, tags)? {
                    Some(obj) => string_pairs(&obj)?,
                    None => Vec::new(),
                };
                host_call(&cx, &h, |host| host.metric_add(&name, kind, value, &tags))?
                    .map_err(|e| Exception::throw_message(&cx, &e))
            },
        ),
    )?;

    // Falls back to `tracing` when no host is attached so that module
    // top-level `console.log` calls during instantiation still work.
    let h = host.clone();
    globals.set(
        "__loadr_log",
        Func::from(move |level: String, message: String| {
            let level = parse_log_level(&level);
            if h.with_host(|host| host.log(level, &message)).is_err() {
                match level {
                    ScriptLogLevel::Debug => {
                        tracing::debug!(target: "loadr_js::script", "{message}")
                    }
                    ScriptLogLevel::Info => tracing::info!(target: "loadr_js::script", "{message}"),
                    ScriptLogLevel::Warn => tracing::warn!(target: "loadr_js::script", "{message}"),
                    ScriptLogLevel::Error => {
                        tracing::error!(target: "loadr_js::script", "{message}")
                    }
                }
            }
        }),
    )?;

    // Returns `undefined` (None) when no host is attached: init-context reads
    // of `__ENV` should not explode.
    let h = host.clone();
    globals.set(
        "__loadr_env",
        Func::from(move |name: String| -> Option<String> {
            h.with_host(|host| host.env_var(&name)).ok().flatten()
        }),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_open",
        Func::from(
            move |cx: Ctx<'js>,
                  path: String,
                  mode: Opt<Value<'js>>|
                  -> rquickjs::Result<Value<'js>> {
                let mode = opt_defined::<String>(&cx, mode)?;
                let bytes = host_call(&cx, &h, |host| host.open_file(&path))?
                    .map_err(|e| Exception::throw_message(&cx, &e))?;
                if mode.as_deref() == Some("b") {
                    let array = TypedArray::new(cx.clone(), bytes)?;
                    Ok(array.into_value())
                } else {
                    Ok(
                        rquickjs::String::from_str(cx.clone(), &String::from_utf8_lossy(&bytes))?
                            .into_value(),
                    )
                }
            },
        ),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_get_var",
        Func::from(
            move |cx: Ctx<'js>, name: String| -> rquickjs::Result<Value<'js>> {
                match host_call(&cx, &h, |host| host.get_var(&name))? {
                    Some(value) => crate::convert::json_to_js(&cx, &value),
                    None => Ok(Value::new_undefined(cx.clone())),
                }
            },
        ),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_set_var",
        Func::from(
            move |cx: Ctx<'js>, name: String, value: Value<'js>| -> rquickjs::Result<()> {
                let json = crate::convert::js_to_json(&cx, &value)?;
                host_call(&cx, &h, |host| host.set_var(&name, json))
            },
        ),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_cookie_get",
        Func::from(
            move |cx: Ctx<'js>, url: String, name: String| -> rquickjs::Result<Option<String>> {
                host_call(&cx, &h, |host| host.cookie_get(&url, &name))
            },
        ),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_cookie_set",
        Func::from(
            move |cx: Ctx<'js>, url: String, name: String, value: String| -> rquickjs::Result<()> {
                host_call(&cx, &h, |host| host.cookie_set(&url, &name, &value))
            },
        ),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_cookies_clear",
        Func::from(move |cx: Ctx<'js>| -> rquickjs::Result<()> {
            host_call(&cx, &h, |host| host.cookies_clear())
        }),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_vu_info",
        Func::from(move |cx: Ctx<'js>| -> rquickjs::Result<Object<'js>> {
            let (vu, iteration, scenario) = host_call(&cx, &h, |host| host.vu_info())?;
            let info = Object::new(cx.clone())?;
            info.set("vu", vu as f64)?;
            info.set("iteration", iteration as f64)?;
            info.set("scenario", scenario)?;
            Ok(info)
        }),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_data_row",
        Func::from(
            move |cx: Ctx<'js>, source: String| -> rquickjs::Result<Value<'js>> {
                let row = host_call(&cx, &h, |host| host.data_row(&source))?
                    .map_err(|e| Exception::throw_message(&cx, &e))?;
                crate::convert::json_to_js(&cx, &row)
            },
        ),
    )?;

    let h = host.clone();
    globals.set(
        "__loadr_http",
        Func::from(
            move |cx: Ctx<'js>,
                  method: String,
                  url: String,
                  body: Opt<Value<'js>>,
                  params: Opt<Object<'js>>|
                  -> rquickjs::Result<Object<'js>> {
                let mut req = HostHttpRequest {
                    method,
                    url,
                    ..Default::default()
                };
                if let Some(b) = body.0 {
                    if !b.is_null() && !b.is_undefined() {
                        req.body = Some(value_to_bytes(&cx, &b)?);
                    }
                }
                if let Some(p) = params.0 {
                    if let Some(headers) = get_defined::<Object>(&cx, &p, "headers")? {
                        req.headers = string_pairs(&headers)?;
                    }
                    let timeout: Value = p.get("timeout")?;
                    req.timeout_ms = parse_timeout(&cx, &timeout)?;
                    if let Some(tags) = get_defined::<Object>(&cx, &p, "tags")? {
                        req.tags = string_pairs(&tags)?;
                    }
                    if let Some(name) = get_defined::<Coerced<String>>(&cx, &p, "name")? {
                        req.name = Some(name.0);
                    }
                }
                let resp = host_call(&cx, &h, move |host| host.http_request(req))?;
                response_to_js(&cx, resp)
            },
        ),
    )?;

    // --- pure helpers (no host) --------------------------------------------
    globals.set(
        "__loadr_digest",
        Func::from(
            move |cx: Ctx<'js>,
                  algo: String,
                  data: Value<'js>,
                  enc: Opt<Value<'js>>|
                  -> rquickjs::Result<String> {
                let enc = opt_defined::<String>(&cx, enc)?;
                let bytes = value_to_bytes(&cx, &data)?;
                let digest =
                    digest_bytes(&algo, &bytes).ok_or_else(|| throw_unknown_algo(&cx, &algo))?;
                encode_digest(&cx, &digest, enc.as_deref())
            },
        ),
    )?;

    globals.set(
        "__loadr_hmac",
        Func::from(
            move |cx: Ctx<'js>,
                  algo: String,
                  secret: Value<'js>,
                  data: Value<'js>,
                  enc: Opt<Value<'js>>|
                  -> rquickjs::Result<String> {
                let enc = opt_defined::<String>(&cx, enc)?;
                let key = value_to_bytes(&cx, &secret)?;
                let payload = value_to_bytes(&cx, &data)?;
                let digest = hmac_bytes(&algo, &key, &payload)
                    .ok_or_else(|| throw_unknown_algo(&cx, &algo))?;
                encode_digest(&cx, &digest, enc.as_deref())
            },
        ),
    )?;

    globals.set(
        "__loadr_random_bytes",
        Func::from(
            move |cx: Ctx<'js>, n: f64| -> rquickjs::Result<TypedArray<'js, u8>> {
                if !n.is_finite() || n < 0.0 || n > MAX_RANDOM_BYTES as f64 {
                    return Err(Exception::throw_range(
                        &cx,
                        &format!("randomBytes size must be between 0 and {MAX_RANDOM_BYTES}"),
                    ));
                }
                let mut buf = vec![0u8; n as usize];
                rand::rng().fill_bytes(&mut buf);
                TypedArray::new(cx.clone(), buf)
            },
        ),
    )?;

    globals.set(
        "__loadr_uuidv4",
        Func::from(move || -> String { uuid::Uuid::new_v4().to_string() }),
    )?;

    globals.set(
        "__loadr_b64encode",
        Func::from(
            move |cx: Ctx<'js>,
                  data: Value<'js>,
                  variant: Opt<Value<'js>>|
                  -> rquickjs::Result<String> {
                let variant = opt_defined::<String>(&cx, variant)?;
                let bytes = value_to_bytes(&cx, &data)?;
                let engine = b64_engine(&cx, variant.as_deref())?;
                Ok(engine.encode(bytes))
            },
        ),
    )?;

    globals.set(
        "__loadr_b64decode",
        Func::from(
            move |cx: Ctx<'js>,
                  data: String,
                  variant: Opt<Value<'js>>|
                  -> rquickjs::Result<String> {
                let variant = opt_defined::<String>(&cx, variant)?;
                let engine = b64_engine(&cx, variant.as_deref())?;
                let bytes = engine.decode(data.trim()).map_err(|e| {
                    Exception::throw_type(&cx, &format!("invalid base64 input: {e}"))
                })?;
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            },
        ),
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// `obj.get(key)` that treats `undefined`/`null` as absent.
fn get_defined<'js, T: FromJs<'js>>(
    ctx: &Ctx<'js>,
    obj: &Object<'js>,
    key: &str,
) -> rquickjs::Result<Option<T>> {
    let value: Value = obj.get(key)?;
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    T::from_js(ctx, value).map(Some)
}

/// Convert a trailing optional argument, treating an explicitly passed
/// `undefined`/`null` the same as a missing argument (unlike bare
/// `Opt<T>`, which only handles missing arguments).
fn opt_defined<'js, T: FromJs<'js>>(
    ctx: &Ctx<'js>,
    arg: Opt<Value<'js>>,
) -> rquickjs::Result<Option<T>> {
    match arg.0 {
        None => Ok(None),
        Some(value) if value.is_undefined() || value.is_null() => Ok(None),
        Some(value) => T::from_js(ctx, value).map(Some),
    }
}

/// Collect an object's own enumerable properties as coerced string pairs.
fn string_pairs(obj: &Object<'_>) -> rquickjs::Result<Vec<(String, String)>> {
    let mut pairs = Vec::new();
    for prop in obj.props::<String, Coerced<String>>() {
        let (key, value) = prop?;
        pairs.push((key, value.0));
    }
    Ok(pairs)
}

fn parse_metric_kind<'js>(ctx: &Ctx<'js>, kind: &str) -> rquickjs::Result<MetricKind> {
    match kind {
        "counter" => Ok(MetricKind::Counter),
        "gauge" => Ok(MetricKind::Gauge),
        "rate" => Ok(MetricKind::Rate),
        "trend" => Ok(MetricKind::Trend),
        other => Err(Exception::throw_type(
            ctx,
            &format!("unknown metric kind `{other}` (expected counter, gauge, rate or trend)"),
        )),
    }
}

fn parse_log_level(level: &str) -> ScriptLogLevel {
    match level {
        "debug" => ScriptLogLevel::Debug,
        "warn" => ScriptLogLevel::Warn,
        "error" => ScriptLogLevel::Error,
        _ => ScriptLogLevel::Info,
    }
}

/// Parse a `timeout` request parameter: a number is milliseconds, a string is
/// a human-friendly duration (`"30s"`, `"500ms"`, bare seconds).
fn parse_timeout<'js>(ctx: &Ctx<'js>, value: &Value<'js>) -> rquickjs::Result<Option<f64>> {
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    if let Some(n) = value.as_number() {
        return Ok(Some(n));
    }
    if let Some(s) = value.as_string() {
        let text = s.to_string()?;
        let dur = loadr_config::Dur::parse(&text).map_err(|e| Exception::throw_type(ctx, &e))?;
        return Ok(Some(dur.as_duration().as_secs_f64() * 1000.0));
    }
    Err(Exception::throw_type(
        ctx,
        "timeout must be a number of milliseconds or a duration string like \"30s\"",
    ))
}

/// Convert a JS value into raw bytes: strings (UTF-8), `Uint8Array`,
/// `ArrayBuffer` and plain arrays of byte values are supported.
fn value_to_bytes<'js>(ctx: &Ctx<'js>, value: &Value<'js>) -> rquickjs::Result<Vec<u8>> {
    if let Some(s) = value.as_string() {
        return Ok(s.to_string()?.into_bytes());
    }
    if let Some(array) = value.as_array() {
        let mut bytes = Vec::with_capacity(array.len());
        for i in 0..array.len() {
            let n: f64 = array.get(i)?;
            if !(0.0..=255.0).contains(&n) {
                return Err(Exception::throw_type(
                    ctx,
                    "array elements must be byte values (0-255)",
                ));
            }
            bytes.push(n as u8);
        }
        return Ok(bytes);
    }
    if value.is_object() {
        if let Ok(ta) = TypedArray::<u8>::from_js(ctx, value.clone()) {
            if let Some(bytes) = ta.as_bytes() {
                return Ok(bytes.to_vec());
            }
        }
        if let Ok(ab) = ArrayBuffer::from_js(ctx, value.clone()) {
            if let Some(bytes) = ab.as_bytes() {
                return Ok(bytes.to_vec());
            }
        }
    }
    Err(Exception::throw_type(
        ctx,
        "expected a string, byte array, Uint8Array or ArrayBuffer",
    ))
}

fn response_to_js<'js>(ctx: &Ctx<'js>, resp: HostHttpResponse) -> rquickjs::Result<Object<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("status", resp.status as f64)?;
    obj.set("status_text", resp.status_text)?;

    let headers = Object::new(ctx.clone())?;
    for (key, value) in resp.headers {
        let key = key.to_ascii_lowercase();
        let merged = match headers.get::<_, Option<String>>(key.as_str())? {
            Some(existing) => format!("{existing}, {value}"),
            None => value,
        };
        headers.set(key, merged)?;
    }
    obj.set("headers", headers)?;

    obj.set("body", String::from_utf8_lossy(&resp.body).into_owned())?;
    obj.set("duration_ms", resp.duration_ms)?;

    let timings = Object::new(ctx.clone())?;
    timings.set("dns_ms", resp.timings.dns_ms)?;
    timings.set("connect_ms", resp.timings.connect_ms)?;
    timings.set("tls_ms", resp.timings.tls_ms)?;
    timings.set("sending_ms", resp.timings.sending_ms)?;
    timings.set("waiting_ms", resp.timings.waiting_ms)?;
    timings.set("receiving_ms", resp.timings.receiving_ms)?;
    timings.set("duration_ms", resp.timings.duration_ms)?;
    timings.set("blocked_ms", resp.timings.blocked_ms)?;
    obj.set("timings", timings)?;

    match resp.error {
        Some(error) => obj.set("error", error)?,
        None => obj.set("error", Value::new_null(ctx.clone()))?,
    }
    obj.set("url", resp.url)?;
    obj.set("protocol", resp.protocol_version)?;
    Ok(obj)
}

fn throw_unknown_algo(ctx: &Ctx<'_>, algo: &str) -> rquickjs::Error {
    Exception::throw_type(
        ctx,
        &format!(
            "unsupported hash algorithm `{algo}` (expected sha256, sha384, sha512, sha1 or md5)"
        ),
    )
}

pub(crate) fn digest_bytes(algo: &str, data: &[u8]) -> Option<Vec<u8>> {
    match algo {
        "sha256" => Some(Sha256::digest(data).to_vec()),
        "sha384" => Some(Sha384::digest(data).to_vec()),
        "sha512" => Some(Sha512::digest(data).to_vec()),
        "sha1" => Some(Sha1::digest(data).to_vec()),
        "md5" => Some(Md5::digest(data).to_vec()),
        _ => None,
    }
}

pub(crate) fn hmac_bytes(algo: &str, key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    macro_rules! mac {
        ($hash:ty) => {{
            let mut mac = SimpleHmac::<$hash>::new_from_slice(key).ok()?;
            mac.update(data);
            Some(mac.finalize().into_bytes().to_vec())
        }};
    }
    match algo {
        "sha256" => mac!(Sha256),
        "sha384" => mac!(Sha384),
        "sha512" => mac!(Sha512),
        "sha1" => mac!(Sha1),
        "md5" => mac!(Md5),
        _ => None,
    }
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing to a String cannot fail.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn encode_digest<'js>(
    ctx: &Ctx<'js>,
    bytes: &[u8],
    encoding: Option<&str>,
) -> rquickjs::Result<String> {
    match encoding.unwrap_or("hex") {
        "hex" => Ok(hex_encode(bytes)),
        "base64" => Ok(STANDARD.encode(bytes)),
        other => Err(Exception::throw_type(
            ctx,
            &format!("unsupported output encoding `{other}` (expected \"hex\" or \"base64\")"),
        )),
    }
}

fn b64_engine<'js>(
    ctx: &Ctx<'js>,
    variant: Option<&str>,
) -> rquickjs::Result<&'static base64::engine::GeneralPurpose> {
    match variant {
        // "s" is k6's "return a string" format flag; we always return strings.
        None | Some("std") | Some("s") => Ok(&STANDARD),
        Some("rawstd") => Ok(&STANDARD_NO_PAD),
        Some("url") => Ok(&URL_SAFE),
        Some("rawurl") => Ok(&URL_SAFE_NO_PAD),
        Some(other) => Err(Exception::throw_type(
            ctx,
            &format!(
                "unsupported base64 variant `{other}` (expected std, rawstd, url, rawurl or s)"
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_known_vectors() {
        assert_eq!(
            hex_encode(&digest_bytes("sha256", b"abc").expect("sha256")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex_encode(&digest_bytes("sha1", b"abc").expect("sha1")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hex_encode(&digest_bytes("md5", b"abc").expect("md5")),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert!(digest_bytes("crc32", b"abc").is_none());
    }

    #[test]
    fn hmac_rfc4231_case1() {
        let key = [0x0bu8; 20];
        let mac = hmac_bytes("sha256", &key, b"Hi There").expect("hmac-sha256");
        assert_eq!(
            hex_encode(&mac),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    #[test]
    fn log_level_parsing() {
        assert_eq!(parse_log_level("debug"), ScriptLogLevel::Debug);
        assert_eq!(parse_log_level("warn"), ScriptLogLevel::Warn);
        assert_eq!(parse_log_level("error"), ScriptLogLevel::Error);
        assert_eq!(parse_log_level("info"), ScriptLogLevel::Info);
        assert_eq!(parse_log_level("anything"), ScriptLogLevel::Info);
    }
}

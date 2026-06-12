//! Conversions between `serde_json::Value` and QuickJS values.
//!
//! Both directions go through the engine's own JSON implementation, which
//! matches the semantics we want: functions, symbols and `undefined` are not
//! representable in JSON and collapse to `Null`/omitted keys.

use rquickjs::{Ctx, Exception, Value};

/// Convert a JSON value into a JS value in `ctx`.
pub fn json_to_js<'js>(ctx: &Ctx<'js>, value: &serde_json::Value) -> rquickjs::Result<Value<'js>> {
    if value.is_null() {
        return Ok(Value::new_null(ctx.clone()));
    }
    let text = serde_json::to_string(value).map_err(|e| {
        Exception::throw_internal(ctx, &format!("failed to serialize argument to JSON: {e}"))
    })?;
    ctx.json_parse(text)
}

/// Convert a JS value into a JSON value.
///
/// Non-JSON values (functions, symbols, `undefined`) become `Null`; object
/// keys holding them are omitted, mirroring `JSON.stringify`. Values that
/// cannot be stringified at all (e.g. cyclic objects) also become `Null`.
pub fn js_to_json<'js>(ctx: &Ctx<'js>, value: &Value<'js>) -> rquickjs::Result<serde_json::Value> {
    if value.is_undefined() || value.is_null() {
        return Ok(serde_json::Value::Null);
    }
    match ctx.json_stringify(value.clone()) {
        Ok(Some(text)) => {
            let text = text.to_string()?;
            Ok(serde_json::from_str(&text).unwrap_or(serde_json::Value::Null))
        }
        Ok(None) => Ok(serde_json::Value::Null),
        Err(rquickjs::Error::Exception) => {
            // e.g. a cyclic object; consume the pending exception and degrade
            // to Null rather than failing the whole call.
            let caught = ctx.catch();
            tracing::warn!(
                "script value could not be converted to JSON: {:?}",
                caught.type_of()
            );
            Ok(serde_json::Value::Null)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rquickjs::{Context, Runtime};
    use serde_json::json;

    fn with_ctx<R>(f: impl for<'js> FnOnce(Ctx<'js>) -> R) -> R {
        let rt = Runtime::new().expect("runtime");
        let ctx = Context::full(&rt).expect("context");
        ctx.with(f)
    }

    #[test]
    fn round_trips_json() {
        with_ctx(|ctx| {
            let input = json!({
                "a": 1,
                "b": [true, null, "x", 2.5],
                "c": {"nested": {"d": -7}},
            });
            let js = json_to_js(&ctx, &input).expect("to js");
            let back = js_to_json(&ctx, &js).expect("to json");
            assert_eq!(back, input);
        });
    }

    #[test]
    fn null_round_trip() {
        with_ctx(|ctx| {
            let js = json_to_js(&ctx, &serde_json::Value::Null).expect("to js");
            assert!(js.is_null());
            let back = js_to_json(&ctx, &js).expect("to json");
            assert_eq!(back, serde_json::Value::Null);
        });
    }

    #[test]
    fn functions_and_undefined_become_null() {
        with_ctx(|ctx| {
            let f: Value = ctx.eval("(function(){})").expect("eval");
            assert_eq!(
                js_to_json(&ctx, &f).expect("to json"),
                serde_json::Value::Null
            );
            let u: Value = ctx.eval("undefined").expect("eval");
            assert_eq!(
                js_to_json(&ctx, &u).expect("to json"),
                serde_json::Value::Null
            );
        });
    }

    #[test]
    fn cyclic_objects_degrade_to_null() {
        with_ctx(|ctx| {
            let v: Value = ctx
                .eval("(function(){const o = {}; o.self = o; return o;})()")
                .expect("eval");
            assert_eq!(
                js_to_json(&ctx, &v).expect("to json"),
                serde_json::Value::Null
            );
        });
    }
}

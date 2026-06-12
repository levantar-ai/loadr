//! Built-in module shims for k6 compatibility.
//!
//! Scripts may `import http from 'k6/http'`, `import { check, sleep } from
//! 'k6'`, etc. The shims simply re-export the globals installed by the
//! prelude, so the import style and the global style behave identically.

use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::module::Declared;
use rquickjs::{Ctx, Error, Module};

/// The built-in modules served by [`K6Loader`].
pub const BUILTIN_MODULES: &[(&str, &str)] = &[
    (
        "k6",
        r#"const g = globalThis;
export const check = (...a) => g.check(...a);
export const sleep = (...a) => g.sleep(...a);
export const group = (...a) => g.group(...a);
export function fail(msg) { throw new Error(msg === undefined ? "test failed" : String(msg)); }
export default { check, sleep, group, fail };
"#,
    ),
    (
        "k6/http",
        r#"const http = globalThis.http;
export default http;
"#,
    ),
    (
        "k6/metrics",
        r#"const g = globalThis;
export const Counter = g.Counter;
export const Gauge = g.Gauge;
export const Rate = g.Rate;
export const Trend = g.Trend;
export default { Counter, Gauge, Rate, Trend };
"#,
    ),
    (
        "k6/crypto",
        r#"const c = globalThis.crypto;
export const sha256 = c.sha256;
export const sha384 = c.sha384;
export const sha512 = c.sha512;
export const sha1 = c.sha1;
export const md5 = c.md5;
export const hmac = c.hmac;
export const randomBytes = c.randomBytes;
export default c;
"#,
    ),
    (
        "k6/encoding",
        r#"const e = globalThis.encoding;
export const b64encode = e.b64encode;
export const b64decode = e.b64decode;
export default e;
"#,
    ),
];

fn builtin_source(name: &str) -> Option<&'static str> {
    BUILTIN_MODULES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, src)| *src)
}

fn known_modules_hint() -> String {
    let names: Vec<&str> = BUILTIN_MODULES.iter().map(|(n, _)| *n).collect();
    format!(
        "unknown module; only built-in modules are available: {}",
        names.join(", ")
    )
}

/// Resolves the k6 built-in module names and rejects everything else with a
/// clear message.
pub struct K6Resolver;

impl Resolver for K6Resolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if builtin_source(name).is_some() {
            Ok(name.to_string())
        } else {
            Err(Error::new_resolving_message(
                base,
                name,
                known_modules_hint(),
            ))
        }
    }
}

/// Loads the k6 built-in module shims.
pub struct K6Loader;

impl Loader for K6Loader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        let source = builtin_source(name)
            .ok_or_else(|| Error::new_loading_message(name, known_modules_hint()))?;
        Module::declare(ctx.clone(), name, source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rquickjs::{Context, Runtime};

    #[test]
    fn resolves_builtins_and_rejects_others() {
        let rt = Runtime::new().expect("runtime");
        let ctx = Context::full(&rt).expect("context");
        ctx.with(|ctx| {
            let mut resolver = K6Resolver;
            for (name, _) in BUILTIN_MODULES {
                let resolved = resolver
                    .resolve(&ctx, "test.js", name, None)
                    .expect("builtin resolves");
                assert_eq!(&resolved, name);
            }
            let err = resolver
                .resolve(&ctx, "test.js", "left-pad", None)
                .expect_err("unknown module rejected");
            let msg = err.to_string();
            assert!(msg.contains("left-pad"), "message names the module: {msg}");
        });
    }
}

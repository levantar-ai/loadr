//! Per-VU cookie jar with RFC 6265-style domain/path matching.

use std::time::SystemTime;

use cookie::Cookie;
use url::Url;

#[derive(Debug, Clone)]
struct StoredCookie {
    name: String,
    value: String,
    domain: String,
    /// True when the cookie came with an explicit Domain attribute
    /// (then subdomains match too).
    domain_wide: bool,
    path: String,
    secure: bool,
    expires: Option<SystemTime>,
}

/// A cookie jar owned by one VU.
#[derive(Debug, Default, Clone)]
pub struct CookieJar {
    cookies: Vec<StoredCookie>,
    /// Automatic handling enabled (from `defaults.http.cookies`).
    pub auto: bool,
}

impl CookieJar {
    pub fn new(auto: bool) -> Self {
        CookieJar {
            cookies: Vec::new(),
            auto,
        }
    }

    /// Store cookies from a `Set-Cookie` header value received for `url`.
    pub fn store_from_header(&mut self, url: &Url, header_value: &str) {
        let Ok(parsed) = Cookie::parse(header_value.to_string()) else {
            return;
        };
        let request_host = url.host_str().unwrap_or("").to_ascii_lowercase();
        let (domain, domain_wide) = match parsed.domain() {
            Some(d) => {
                let d = d.trim_start_matches('.').to_ascii_lowercase();
                // Reject cookies for unrelated domains.
                if !domain_matches(&request_host, &d, true) {
                    return;
                }
                (d, true)
            }
            None => (request_host.clone(), false),
        };
        let path = parsed
            .path()
            .map(str::to_string)
            .unwrap_or_else(|| default_path(url));
        let expires = match parsed.expires() {
            Some(cookie::Expiration::DateTime(dt)) => Some(SystemTime::from(dt)),
            _ => parsed.max_age().and_then(|ma| {
                let std: Result<std::time::Duration, _> = ma.try_into();
                std.ok().map(|d| SystemTime::now() + d)
            }),
        };
        // Max-Age <= 0 / expired => deletion.
        let expired = expires.map(|e| e <= SystemTime::now()).unwrap_or(false);
        let name = parsed.name().to_string();
        self.cookies
            .retain(|c| !(c.name == name && c.domain == domain && c.path == path));
        if !expired {
            self.cookies.push(StoredCookie {
                name,
                value: parsed.value().to_string(),
                domain,
                domain_wide,
                path,
                secure: parsed.secure().unwrap_or(false),
                expires,
            });
        }
    }

    /// Build a `Cookie:` header value for a request to `url`.
    pub fn header_for(&mut self, url: &Url) -> Option<String> {
        let now = SystemTime::now();
        self.cookies
            .retain(|c| c.expires.map(|e| e > now).unwrap_or(true));
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        let path = if url.path().is_empty() {
            "/"
        } else {
            url.path()
        };
        let https = url.scheme() == "https" || url.scheme() == "wss";
        let mut pairs: Vec<String> = Vec::new();
        for c in &self.cookies {
            if !domain_matches(&host, &c.domain, c.domain_wide) {
                continue;
            }
            if !path_matches(path, &c.path) {
                continue;
            }
            if c.secure && !https {
                continue;
            }
            pairs.push(format!("{}={}", c.name, c.value));
        }
        if pairs.is_empty() {
            None
        } else {
            Some(pairs.join("; "))
        }
    }

    /// Manually set a cookie (host-only, path `/`).
    pub fn set(&mut self, url: &Url, name: &str, value: &str) {
        let domain = url.host_str().unwrap_or("").to_ascii_lowercase();
        self.cookies
            .retain(|c| !(c.name == name && c.domain == domain && c.path == "/"));
        self.cookies.push(StoredCookie {
            name: name.to_string(),
            value: value.to_string(),
            domain,
            domain_wide: false,
            path: "/".to_string(),
            secure: false,
            expires: None,
        });
    }

    /// Get a cookie value applicable to `url`.
    pub fn get(&self, url: &Url, name: &str) -> Option<String> {
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        let path = if url.path().is_empty() {
            "/"
        } else {
            url.path()
        };
        self.cookies
            .iter()
            .filter(|c| {
                c.name == name
                    && domain_matches(&host, &c.domain, c.domain_wide)
                    && path_matches(path, &c.path)
            })
            .map(|c| c.value.clone())
            .next()
    }

    pub fn clear(&mut self) {
        self.cookies.clear();
    }

    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }
}

fn domain_matches(request_host: &str, cookie_domain: &str, domain_wide: bool) -> bool {
    if request_host == cookie_domain {
        return true;
    }
    if !domain_wide {
        return false;
    }
    request_host
        .strip_suffix(cookie_domain)
        .map(|prefix| prefix.ends_with('.'))
        .unwrap_or(false)
}

fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    if request_path == cookie_path {
        return true;
    }
    if let Some(rest) = request_path.strip_prefix(cookie_path) {
        return cookie_path.ends_with('/') || rest.starts_with('/');
    }
    false
}

fn default_path(url: &Url) -> String {
    let p = url.path();
    match p.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(idx) => p[..idx].to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).expect("url")
    }

    #[test]
    fn basic_set_and_send() {
        let mut jar = CookieJar::new(true);
        jar.store_from_header(&url("https://example.com/login"), "session=abc123; Path=/");
        let header = jar.header_for(&url("https://example.com/account")).unwrap();
        assert_eq!(header, "session=abc123");
    }

    #[test]
    fn host_only_does_not_match_subdomain() {
        let mut jar = CookieJar::new(true);
        jar.store_from_header(&url("https://example.com/"), "a=1");
        assert!(jar.header_for(&url("https://sub.example.com/")).is_none());
    }

    #[test]
    fn domain_attribute_matches_subdomains() {
        let mut jar = CookieJar::new(true);
        jar.store_from_header(&url("https://example.com/"), "a=1; Domain=example.com");
        assert_eq!(
            jar.header_for(&url("https://sub.example.com/")).unwrap(),
            "a=1"
        );
    }

    #[test]
    fn rejects_cookie_for_unrelated_domain() {
        let mut jar = CookieJar::new(true);
        jar.store_from_header(&url("https://example.com/"), "a=1; Domain=evil.com");
        assert!(jar.is_empty());
    }

    #[test]
    fn path_scoping() {
        let mut jar = CookieJar::new(true);
        jar.store_from_header(&url("https://e.com/"), "a=1; Path=/admin");
        assert!(jar.header_for(&url("https://e.com/")).is_none());
        assert!(jar.header_for(&url("https://e.com/admin/users")).is_some());
        assert!(jar
            .header_for(&url("https://e.com/administrator"))
            .is_none());
    }

    #[test]
    fn secure_cookies_not_sent_over_http() {
        let mut jar = CookieJar::new(true);
        jar.store_from_header(&url("https://e.com/"), "a=1; Secure");
        assert!(jar.header_for(&url("http://e.com/")).is_none());
        assert!(jar.header_for(&url("https://e.com/")).is_some());
    }

    #[test]
    fn max_age_zero_deletes() {
        let mut jar = CookieJar::new(true);
        jar.store_from_header(&url("https://e.com/"), "a=1");
        assert_eq!(jar.len(), 1);
        jar.store_from_header(&url("https://e.com/"), "a=1; Max-Age=0");
        assert!(jar.header_for(&url("https://e.com/")).is_none());
    }

    #[test]
    fn overwrite_same_name_domain_path() {
        let mut jar = CookieJar::new(true);
        jar.store_from_header(&url("https://e.com/"), "a=1");
        jar.store_from_header(&url("https://e.com/"), "a=2");
        assert_eq!(jar.len(), 1);
        assert_eq!(jar.header_for(&url("https://e.com/")).unwrap(), "a=2");
    }

    #[test]
    fn manual_get_set() {
        let mut jar = CookieJar::new(true);
        jar.set(&url("https://e.com/x"), "tok", "v1");
        assert_eq!(jar.get(&url("https://e.com/y"), "tok").unwrap(), "v1");
        jar.clear();
        assert!(jar.is_empty());
    }
}

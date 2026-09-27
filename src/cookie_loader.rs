use std::fs::File;
use std::io::Read;

use cookie::time::OffsetDateTime;
use cookie::{Expiration, time};
use cookie_store::RawCookie;
use url::Url;

use crate::NCMEAPI_DOMAIN;

#[derive(Debug)]
pub enum LoadErr {
    IO(std::io::Error),
    Store(cookie_store::Error),
    Time(time::error::ComponentRange),
    Url(url::ParseError),
}

impl std::fmt::Display for LoadErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadErr::IO(e) => write!(f, "IO: {e}"),
            LoadErr::Store(e) => write!(f, "Store: {e}"),
            LoadErr::Time(e) => write!(f, "Time: {e}"),
            LoadErr::Url(e) => write!(f, "Url: {e}"),
        }
    }
}

impl std::error::Error for LoadErr {}

impl From<std::io::Error> for LoadErr {
    fn from(e: std::io::Error) -> Self {
        LoadErr::IO(e)
    }
}
impl From<cookie_store::Error> for LoadErr {
    fn from(e: cookie_store::Error) -> Self {
        LoadErr::Store(e)
    }
}
impl From<time::error::ComponentRange> for LoadErr {
    fn from(e: time::error::ComponentRange) -> Self {
        LoadErr::Time(e)
    }
}
impl From<url::ParseError> for LoadErr {
    fn from(e: url::ParseError) -> Self {
        LoadErr::Url(e)
    }
}

pub fn parse_cookie_txt(path: &str) -> Result<reqwest_cookie_store::CookieStore, LoadErr> {
    let mut cookies_txt = File::open(path)?;
    let mut content = String::new();
    let mut store = reqwest_cookie_store::CookieStore::new();
    cookies_txt.read_to_string(&mut content)?;
    let cookies = cookiestxt_rs::Cookies::try_from(content.as_str()).unwrap();
    for c in cookies.iter() {
        let mut builder = RawCookie::build((c.name.as_str(), c.value.as_str()))
            .path(c.path.as_str())
            .secure(c.https_only)
            .http_only(c.http_only);
        let domain = if c.domain.ends_with("163.com") {
            NCMEAPI_DOMAIN
        } else {
            c.domain.as_str()
        };

        if !domain.is_empty() {
            builder = builder.domain(domain);
        }
        if c.expires > 0 {
            builder = builder.expires(Expiration::from(
                OffsetDateTime::from_unix_timestamp(c.expires as i64).unwrap(),
            ));
        }
        let raw = builder.build();
        let url = Url::parse(&format!("https://{}", domain)).unwrap();

        match cookie_store::Cookie::try_from_raw_cookie(&raw, &url) {
            Ok(cookie) => match store.insert(cookie.into_owned(), &url) {
                Ok(_) => {}
                Err(e) => eprintln!("skip {}: {}", c.name, e),
            },
            Err(e) => eprintln!("skip {}: {}", c.name, e),
        }
    }
    Ok(store)
}

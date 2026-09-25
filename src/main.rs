mod api;
mod cookie_loader;
mod util;

use api::{get_album_id, get_album_info};
use reqwest_cookie_store::CookieStoreMutex;

use clap::Parser;
use reqwest::Client;

use crate::{api::AudioQuality, cookie_loader::parse_cookie_txt};
pub use api::{NCMEAPI_DOMAIN, NCMEAPI_URL};

/// A netease cloud music album downloader
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // Album URL / 专辑地址
    album_url: String,

    // Cookies TXT / Cookies TXT文件路径
    #[arg(long, short)]
    cookies: Option<String>,

    // Audio Quality / 音质
    #[arg(long, short, default_value = "exhigh")]
    quality: AudioQuality,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let mut client_builder = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(10));
    let jar;
    match args.cookies {
        Some(path_to_cookies) => {
            eprintln!("COOKIES_LOADER <- Some({})", path_to_cookies);
            match parse_cookie_txt(&path_to_cookies) {
                Ok(store) => {
                    eprintln!("COOKIES_LOADER -> Ok(()) ");
                    eprintln!(
                        "COOKIES <- {:?}",
                        store.clone().iter_unexpired().collect::<Vec<_>>()
                    );
                    jar = std::sync::Arc::new(CookieStoreMutex::new(store));
                    client_builder = client_builder.cookie_provider(jar);
                }
                Err(e) => {
                    eprintln!("COOKIES_LOADER -> Err({})", e);
                    eprintln!("COOKIES <- None")
                }
            }
        }
        None => eprintln!("COOKIES <- None"),
    }
    let client = client_builder.build().ok().expect("CLIENT_BUILD -> FAIL");

    let album_url = args.album_url;
    eprintln!("ALBUM_URL <- {}", album_url);
    let album_id = get_album_id(client.clone(), &album_url).await;
    match album_id {
        Some(album_id) => eprintln!("ALBUM_ID -> {}", album_id),
        None => eprintln!("ALBUM_ID -> FAIL"),
    }
    let album_id = album_id.unwrap();
    eprintln!("ALBUM_INFO_API <- {}", album_id);
    let album_info = get_album_info(client.clone(), album_id, args.quality).await.unwrap();
    println!("{:#?}", album_info);
}

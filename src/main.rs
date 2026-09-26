mod api;
mod cookie_loader;
mod util;

use reqwest_cookie_store::CookieStoreMutex;

use clap::Parser;
use reqwest::Client;

use crate::{
    api::{AudioQuality, NCMAPI},
    cookie_loader::parse_cookie_txt,
    util::make_album,
};
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

    #[arg(long, short = 'n', default_value_t = 4)]
    concurrent: usize,

    #[arg(long, short = 'a', default_value_t = 3)]
    attempts: usize,

    #[arg(long, short, default_value = "./out")]
    output: String,
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
                    eprintln!("COOKIES <- {{...}}");
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
    let ncmapi = NCMAPI::new(client, args.attempts, args.concurrent);
    let album_id = ncmapi.get_album_id(&album_url).await;
    match album_id {
        Some(album_id) => eprintln!("ALBUM_ID -> {}", album_id),
        None => eprintln!("ALBUM_ID -> FAIL"),
    }
    let album_id = album_id.unwrap();
    eprintln!("ALBUM_INFO_API <- {}", album_id);
    let album_info = ncmapi.get_album_info(album_id, args.quality).await.unwrap();
    println!("{:#?}", album_info);
    let downloaded = ncmapi.download_album(&album_info, args.output).await;
    make_album(album_info, downloaded);
}

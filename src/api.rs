use crate::util::{AlbumSavePath, extract_album_id, get_disc_subtitle, url_filename};

use reqwest::Client;
use tokio::fs;
use url::Url;

pub const NCMEAPI_DOMAIN: &str = "ncmapi.xslimenb.eu.org";
pub const NCMEAPI_URL: &str = "https://ncmapi.xslimenb.eu.org";

use serde::{Deserialize, Deserializer, Serialize, de::Visitor};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{collections::HashMap, fmt, path::PathBuf, time::Duration};

pub async fn resolve_short_url(client: Client, input: &str, attempts: usize) -> Option<Url> {
    let resp = with_retry(
        || {
            let c = client.clone();
            let i = input;
            async move { c.get(i).send().await }
        },
        attempts,
    )
    .await
    .ok()?;
    Some(resp.url().clone())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
#[value(rename_all = "lower")]
pub enum AudioQuality {
    Standard, // standard
    Higher,   // higher
    ExHigh,   // exhigh
    Lossless, // lossless
    HiRes,    // hires
    JyEffect, // jyeffect
    Sky,      // sky
    Dolby,    // dolby
    JyMaster, // jymaster
}

impl AudioQuality {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Standard => "标准",
            Self::Higher => "较高",
            Self::ExHigh => "极高",
            Self::Lossless => "无损",
            Self::HiRes => "Hi-Res",
            Self::JyEffect => "高清环绕声",
            Self::Sky => "沉浸环绕声",
            Self::Dolby => "杜比全景声",
            Self::JyMaster => "超清母带",
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Higher => "higher",
            Self::ExHigh => "exhigh",
            Self::Lossless => "lossless",
            Self::HiRes => "hires",
            Self::JyEffect => "jyeffect",
            Self::Sky => "sky",
            Self::Dolby => "dolby",
            Self::JyMaster => "jymaster",
        }
    }
}

impl fmt::Display for AudioQuality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AlbumInfo {
    pub album: AlbumDetail,
    pub songs: Vec<SongDetailAl>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct AlbumDetail {
    pub name: String, // META album
    #[serde(rename = "picUrl")]
    pub pic_url: String, // FILE cover.EXT
    #[serde(rename = "publishTime")]
    pub publish_time: u64, // META date (millseconds timestamp UTC)
    pub company: String, // META copyright ("" means Nothing)
    pub artists: Vec<ArtistDetail>, // META album_artist
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ArtistDetail {
    pub id: u64,
    pub name: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiscInfo {
    /// Disc Number
    pub number: u32,
    /// Total Disc
    pub total: u32,
    /// Disc Name
    pub subtitle: String,
    /// Raw (serde fill it)
    pub raw: String,
}

impl<'de> Deserialize<'de> for DiscInfo {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(RawVisitor)
    }
}

struct RawVisitor;

impl<'de> Visitor<'de> for RawVisitor {
    type Value = DiscInfo;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("null, a number, or a string")
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<DiscInfo, E> {
        Ok(DiscInfo::default())
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<DiscInfo, E> {
        Ok(DiscInfo::default())
    }
    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<DiscInfo, D::Error> {
        DiscInfo::deserialize(d)
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<DiscInfo, E> {
        Ok(DiscInfo {
            raw: v.to_string(),
            ..Default::default()
        })
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<DiscInfo, E> {
        Ok(DiscInfo {
            raw: v.to_string(),
            ..Default::default()
        })
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<DiscInfo, E> {
        Ok(DiscInfo {
            raw: v.to_owned(),
            ..Default::default()
        })
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<DiscInfo, E> {
        Ok(DiscInfo {
            raw: v,
            ..Default::default()
        })
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct SongDetail {
    pub id: u64,
    pub name: String,
    pub ar: Vec<SongArtist>,
    pub al: SongAlbum,
    pub no: u64,
    pub cd: DiscInfo,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct SongDetailAl {
    #[serde(flatten)]
    pub d: SongDetail,

    pub privilege: Privilege,

    #[serde(skip)]
    pub url: Option<SongUrlData>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct SongArtist {
    pub id: u64,
    pub name: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct Privilege {
    #[serde(rename = "plLevel")]
    pub pl_level: AudioQuality, // Play Quality
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct SongAlbum {
    #[serde(rename = "picUrl")]
    pub pic_url: String, // FILE cover.EXT
}

#[allow(dead_code)]
pub fn resolve_discs(songs: &mut [SongDetailAl]) {
    if songs.is_empty() {
        return;
    }

    let mut cur_disc_number = 1;
    let mut cur_track_number = 1;

    // resort disc/track number, extract disc subtitle
    songs[0].d.cd.subtitle = get_disc_subtitle(songs[0].d.cd.raw.clone()).to_owned();
    songs[0].d.cd.number = cur_disc_number;
    songs[0].d.no = cur_track_number;
    for i in 1..songs.len() {
        if songs[i].d.cd.raw != songs[i - 1].d.cd.raw {
            cur_disc_number += 1;
            cur_track_number = 1;
        }
        songs[i].d.cd.subtitle = get_disc_subtitle(songs[i].d.cd.raw.clone()).to_owned();
        songs[i].d.cd.number = cur_disc_number;
        songs[i].d.no = cur_track_number;
        cur_track_number += 1;
    }
    for x in songs {
        x.d.cd.total = cur_disc_number;
    }
}

#[derive(Debug, Deserialize)]
pub struct SongDetails {
    pub songs: Vec<SongDetail>,
    pub privileges: Vec<Privilege>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct SongUrlData {
    pub id: u64,
    pub url: String,
    #[serde(rename = "type")]
    pub file_type: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SongUrls {
    data: Vec<SongUrlData>,
}

async fn with_retry<F, Fut, T, E>(mut f: F, retries: usize) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let mut attempt = 0;
    loop {
        match f().await {
            Ok(v) => return Ok(v),
            Err(_) if attempt < retries => {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(300 * (1 << attempt).min(8))).await;
            }
            Err(e) => return Err(e),
        }
    }
}

fn make_progress(mp: &MultiProgress, name: &str, total: Option<u64>) -> ProgressBar {
    match total {
        Some(total) => {
            let pb = mp.add(ProgressBar::new(total));
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} {msg:25!} [{bar:40.cyan/blue}] \
                     {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta})",
                )
                .unwrap()
                .progress_chars("=>-"),
            );
            pb.set_message(name.to_string());
            pb
        }
        None => {
            let pb = mp.add(ProgressBar::new_spinner());
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} {msg:25!} {bytes} ({bytes_per_sec})",
                )
                .unwrap(),
            );
            pb.set_message(name.to_string());
            pb.enable_steady_tick(Duration::from_millis(100));
            pb
        }
    }
}

async fn download_one(
    attempts: usize,
    client: reqwest::Client,
    save_path: PathBuf,
    file_name: String,
    url: String,
    mp: MultiProgress,
) -> Result<(String, u64), Box<dyn std::error::Error>> {
    let max_attempts = attempts;
    let mut attempt = 0;

    loop {
        // 每次 attempt 都重新请求 + 重建进度条
        let resp = client.get(url.clone()).send().await?.error_for_status()?;
        let total = resp.content_length();
        let pb = make_progress(&mp, &file_name, total);

        match download_body(resp, &save_path, &file_name, &pb).await {
            Ok((name, bytes)) => {
                pb.finish_with_message(format!("OK   {name} ({bytes} bytes)"));
                return Ok((name, bytes));
            }
            Err(_) if attempt < max_attempts => {
                attempt += 1;
                pb.abandon_with_message(format!(
                    "RETRY {file_name} (attempt {attempt}/{max_attempts})"
                ));
                // 清掉半成品，避免下次写到损坏文件
                let _ = tokio::fs::remove_file(save_path.join(&file_name)).await;
                tokio::time::sleep(Duration::from_millis(1000 * attempt as u64)).await;
            }
            Err(e) => {
                pb.abandon_with_message(format!("FAIL {file_name}"));
                return Err(e);
            }
        }
    }
}

use futures::{StreamExt, stream};
use tokio::io::AsyncWriteExt;

async fn download_body(
    resp: reqwest::Response,
    save_path: &PathBuf,
    file_name: &str,
    pb: &ProgressBar,
) -> Result<(String, u64), Box<dyn std::error::Error>> {
    let mut file = fs::File::create(save_path.join(file_name)).await?;

    let mut stream = resp.bytes_stream();
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        pb.inc(chunk.len() as u64); // ← 关键：每收到一块就推进
    }
    file.flush().await?;

    Ok((file_name.to_string(), downloaded))
}

fn push_task(map: &mut HashMap<String, String>, url_str: String) {
    map.insert(url_filename(url_str.clone()), url_str);
}

#[derive(Clone)]
pub struct AlbumDownloaded {
    pub paths: AlbumSavePath,
    pub downloads: HashMap<String, String>,
}

impl AlbumDownloaded {
    fn new(info: &AlbumInfo, save_path: String) -> Self {
        // init folders
        let album_path = PathBuf::from(save_path).join(info.album.name.clone());
        let paths = AlbumSavePath::new(album_path);
        paths.init();
        // prepare download tasks
        let mut downloads = HashMap::new();
        push_task(&mut downloads, info.album.pic_url.clone());
        for x in &info.songs {
            push_task(&mut downloads, x.url.clone().unwrap().url);
            push_task(&mut downloads, x.d.al.pic_url.clone());
        }
        AlbumDownloaded { paths, downloads }
    }
}

pub struct NCMAPI {
    client: Client,
    attempts: usize,
    concurrent: usize,
}

impl NCMAPI {
    pub fn new(client: Client, attempts: usize, concurrent: usize) -> Self {
        Self {
            client,
            attempts,
            concurrent,
        }
    }

    pub async fn get_album_id(&self, url: &str) -> Option<u64> {
        // if short link passed
        let binding;
        let url = if !url.contains("music.163.com") {
            match resolve_short_url(self.client.clone(), url, self.attempts).await {
                Some(real_url) => {
                    binding = real_url.to_string();
                    println!("EXPANDED -> {}", binding);
                    &binding
                }
                None => url,
            }
        } else {
            url
        };
        extract_album_id(url)
    }

    async fn refresh_album_tracks(
        &self,
        songs: &mut [SongDetailAl],
        quality: AudioQuality,
    ) -> Option<()> {
        let track_ids = songs.iter().map(|x| x.d.id).collect::<Vec<_>>();
        let trackids_str = track_ids
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let detail_address = format!("{}/song/detail?ids={}", NCMEAPI_URL, trackids_str);
        let get_url_address = format!(
            "{}/song/url/v1?id={}&level={}&os=pc",
            NCMEAPI_URL,
            trackids_str,
            quality.as_str()
        );

        let (details, urls) = tokio::try_join!(
            async {
                with_retry(
                    || {
                        let c = self.client.clone();
                        let a = detail_address.clone();
                        async move { c.get(a).send().await?.json::<SongDetails>().await }
                    },
                    self.attempts,
                )
                .await
            },
            async {
                with_retry(
                    || {
                        let c = self.client.clone();
                        let a = get_url_address.clone();
                        async move { c.get(a).send().await?.json::<SongUrls>().await }
                    },
                    self.attempts,
                )
                .await
            }
        )
        .ok()?;
        for (i, data) in songs.iter_mut().enumerate() {
            // data.d = details.songs[i].clone();
            // only rewrite the cover
            data.d.al.pic_url = details.songs[i].al.pic_url.clone();
            data.privilege = details.privileges[i].clone();
            data.url = Some(
                urls.data
                    .iter()
                    .find(|x| x.id == data.d.id)
                    .unwrap()
                    .clone(),
            );
        }
        Some(())
    }

    pub async fn get_album_info(&self, album_id: u64, quality: AudioQuality) -> Option<AlbumInfo> {
        let api_address = format!("{}/album?id={}&os=pc", NCMEAPI_URL, album_id);
        let mut res: AlbumInfo = with_retry(
            || {
                let c = self.client.clone();
                let a = api_address.clone();
                async move { c.get(a).send().await?.json::<AlbumInfo>().await }
            },
            self.attempts,
        )
        .await
        .ok()?;
        self.refresh_album_tracks(&mut res.songs, quality).await;
        resolve_discs(&mut res.songs);
        Some(res)
    }

    pub async fn download_album(&self, info: &AlbumInfo, save_path: String) -> AlbumDownloaded {
        let dat = AlbumDownloaded::new(info, save_path);
        // start download
        let mp = MultiProgress::new();
        // make tasks
        let tasks = dat.downloads.clone().into_iter().map(|(name, url)| {
            let client = self.client.clone();
            let mp = mp.clone();
            let path = dat.paths.tmp_path.clone();
            async move { download_one(self.attempts, client, path, name, url, mp).await }
        });
        // parallel run them
        let results: Vec<Result<(String, u64), Box<dyn std::error::Error>>> = stream::iter(tasks)
            .buffer_unordered(self.concurrent)
            .collect()
            .await;
        let mut ok = 0;
        let mut err = 0;
        for r in &results {
            match r {
                Ok((_, _)) => ok += 1,
                Err(_) => err += 1,
            }
        }
        mp.println(format!("FINISH: {ok} SUCC, {err} FAIL"))
            .unwrap();
        for r in results {
            if let Err(e) = r {
                mp.println(format!("FAIL {e:#}")).unwrap();
            }
        }
        dat.clone()
    }
}

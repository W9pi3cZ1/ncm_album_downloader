use crate::util::extract_album_id;

use reqwest::Client;
use url::Url;

pub const NCMEAPI_DOMAIN: &str = "ncmapi.xslimenb.eu.org";
pub const NCMEAPI_URL: &str = "https://ncmapi.xslimenb.eu.org";

use serde::{Deserialize, Deserializer, Serialize, de::Visitor};

use std::fmt;

pub async fn resolve_short_url(client: Client, input: &str) -> Option<Url> {
    let resp = client.get(input).send().await.ok()?;
    Some(resp.url().clone())
}

pub async fn get_album_id(client: Client, url: &str) -> Option<u64> {
    // if short link passed
    let binding;
    let url = if !url.contains("music.163.com") {
        match resolve_short_url(client, url).await {
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
    pub company: String, // META copyright
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
    pub number: Option<u32>,
    /// Total Disc
    pub total: Option<u32>,
    /// Disc Name
    pub subtitle: Option<String>,
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

fn get_disc_subtitle(disc_raw: String) -> String {
    let mut disc_name = disc_raw.as_str();
    if disc_name.starts_with(|c: char| c.is_ascii_digit()) {
        disc_name = disc_name.trim_start_matches(|c: char| c.is_ascii_digit());
        if let Some(rest) = disc_name.strip_prefix('/') {
            let after_slash_digits = rest.trim_start_matches(|c: char| c.is_ascii_digit());
            if after_slash_digits.len() != rest.len() {
                disc_name = after_slash_digits;
            }
        }
    }
    disc_name.to_owned()
}

#[allow(dead_code)]
pub fn resolve_discs(songs: &mut [SongDetailAl]) {
    if songs.is_empty() {
        return;
    }

    let mut cur_disc_number = 1;
    let mut cur_track_number = 1;

    // resort disc/track number, extract disc subtitle
    songs[0].d.cd.subtitle = Some(get_disc_subtitle(songs[0].d.cd.raw.clone()).to_owned());
    songs[0].d.cd.number = Some(cur_disc_number);
    songs[0].d.no = cur_track_number;
    for i in 1..songs.len() {
        if songs[i].d.cd.raw != songs[i - 1].d.cd.raw {
            cur_disc_number += 1;
            cur_track_number = 1;
        }
        songs[i].d.cd.subtitle = Some(get_disc_subtitle(songs[i].d.cd.raw.clone()).to_owned());
        songs[i].d.cd.number = Some(cur_disc_number);
        songs[i].d.no = cur_track_number;
        cur_track_number += 1;
    }
    for x in songs {
        x.d.cd.total = Some(cur_disc_number);
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
    id: u64,
    url: String,
    #[serde(rename = "type")]
    file_type: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SongUrls {
    data: Vec<SongUrlData>,
}

async fn refresh_album_tracks(
    client: Client,
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
        "{}/song/url/v1?id={}&level={}",
        NCMEAPI_URL,
        trackids_str,
        quality.as_str()
    );
    let (details, urls) = tokio::try_join!(
        async {
            client
                .get(detail_address)
                .send()
                .await?
                .json::<SongDetails>()
                .await
        },
        async {
            client
                .get(get_url_address)
                .send()
                .await?
                .json::<SongUrls>()
                .await
        },
    )
    .ok()?;
    for (i, data) in songs.iter_mut().enumerate() {
        data.d = details.songs[i].clone();
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

pub async fn get_album_info(
    client: Client,
    album_id: u64,
    quality: AudioQuality,
) -> Option<AlbumInfo> {
    let api_address = format!("{}/album?id={}&os=pc", NCMEAPI_URL, album_id);
    let resp = client.get(api_address).send().await.ok()?;
    let mut res: AlbumInfo = resp.json().await.ok()?;
    refresh_album_tracks(client, &mut res.songs, quality).await;
    resolve_discs(&mut res.songs);
    Some(res)
}

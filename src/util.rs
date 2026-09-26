use std::{fs, path::PathBuf};

use url::Url;

use crate::api::{AlbumDownloaded, AlbumInfo, DiscInfo};

use cookie::time::OffsetDateTime;
use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    picture::{MimeType, Picture, PictureType},
    probe::Probe,
    tag::{Accessor, ItemKey, Tag},
};
use std::path::Path;

pub fn pathname(url: &str) -> String {
    // Remove scheme://
    let (had_scheme, s) = match url.split_once("://") {
        Some((_, rest)) => (true, rest),
        None => (false, url),
    };

    // Remove host
    let s: &str = if had_scheme {
        s.find('/').map(|i| &s[i..]).unwrap_or("/")
    } else if let Some(i) = s.find('/') {
        let head = &s[..i];
        if !head.is_empty() && head.contains('.') {
            &s[i..]
        } else {
            s
        }
    } else {
        s
    };

    // Use fragment if s has '#'
    let s = s.split_once('#').map(|(_, f)| f).unwrap_or(s);

    // Remove '/' and 'm/' prefix
    let s = s.trim_start_matches('/');
    let s = s.strip_prefix("m/").unwrap_or(s);

    // Get path / query
    let (path, query) = match s.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (s, None),
    };

    // Get id property from query
    let id = query.and_then(|q| {
        q.split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == "id")
            .map(|(_, v)| v)
    });

    // Splice together
    match id {
        Some(v) => format!("/{}/{}", path.trim_end_matches('/'), v),
        None => format!("/{}", path.trim_end_matches("/")),
    }
}

pub fn extract_album_id(url: &str) -> Option<u64> {
    let pathname = pathname(url);
    eprintln!("PATHNAME -> {}", pathname);
    let id_str = match pathname.split_once("/album/") {
        Some((_, rest)) => rest,
        None => return None,
    };
    match id_str.parse::<u64>() {
        Ok(id) => Some(id),
        Err(_) => None,
    }
}

pub fn get_disc_subtitle(disc_raw: String) -> String {
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

fn sanitize_filename_component(name: &str) -> String {
    let mut out = String::with_capacity(name.len());

    for ch in name.chars() {
        match ch {
            '/' => out.push('／'),
            '\\' => out.push('＼'),
            ':' => out.push('：'),
            '*' => out.push('＊'),
            '?' => out.push('？'),
            '"' => out.push('＂'),
            '<' => out.push('＜'),
            '>' => out.push('＞'),
            '|' => out.push('｜'),
            c if c.is_control() => out.push('_'),
            c => out.push(c),
        }
    }

    // Windows 下文件名末尾不能是空格或点
    let trimmed = out.trim().trim_end_matches('.').trim_end();
    if trimmed.is_empty() {
        "_".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[derive(Clone)]
pub struct AlbumSavePath {
    pub album_path: PathBuf,
    pub tmp_path: PathBuf,
}

impl AlbumSavePath {
    pub fn new(album_path: PathBuf) -> AlbumSavePath {
        AlbumSavePath {
            album_path: album_path.clone(),
            tmp_path: album_path.join("tmp"),
        }
    }
    pub fn init(&self) {
        fs::create_dir_all(self.album_path.clone()).unwrap();
        fs::create_dir_all(self.tmp_path.clone()).unwrap();
    }
}

pub fn url_filename(url_str: String) -> String {
    let url: Url = Url::parse(&url_str).unwrap();
    url.path_segments().unwrap().last().unwrap().to_owned()
}

fn get_tmp_path(url: String, downloaded: &AlbumDownloaded) -> PathBuf {
    downloaded.paths.tmp_path.join(url_filename(url))
}

#[allow(clippy::too_many_arguments)]
fn write_audio_metadata(
    path: &Path,
    title: &str,
    artists: &[String],
    album: &str,
    album_artist: &str,
    track_number: u32,
    disc_number: u32,
    total_discs: u32,
    total_tracks: u32,
    copyright: Option<String>,
    release_date: Option<String>,
    cover_data: Option<&[u8]>,
    cover_mime: Option<MimeType>,
    lyric: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. 读取文件（lofty 自动探测格式）
    let mut tagged_file = Probe::open(path)?.read()?;

    // 2. 若文件原本没有标签，创建一个原生类型的空标签
    if tagged_file.primary_tag().is_none() {
        let tag_type = tagged_file.primary_tag_type();
        tagged_file.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged_file
        .primary_tag_mut()
        .ok_or("failed to create/find primary tag")?;

    // 抹除可能损坏的封面
    while !tag.pictures().is_empty() {
        tag.remove_picture(0);
    }

    // 3. 基础字段
    tag.set_title(title.to_owned());
    tag.set_album(album.to_owned());
    // 网易云 API 里 ar 是数组，这里用 "; "
    tag.set_artist(artists.join("; "));

    if track_number > 0 {
        tag.set_track(track_number);
    }
    if disc_number > 0 {
        tag.set_disk(disc_number);
    }

    // 4. 扩展字段（用 ItemKey 兼容多格式）
    if total_tracks > 0 {
        tag.insert_text(ItemKey::TrackTotal, total_tracks.to_string());
    }
    if total_discs > 1 {
        tag.insert_text(ItemKey::DiscTotal, total_discs.to_string());
    }
    if !album_artist.is_empty() {
        tag.insert_text(ItemKey::AlbumArtist, album_artist.to_owned());
    }
    if let Some(d) = &release_date {
        // 完整日期 → 标准字段（ID3v2: TDRC；Vorbis: DATE；MP4: ©day）
        tag.insert_text(ItemKey::RecordingDate, d.clone());

        // 同时写一份纯年份，兼容只认 YEAR 的老播放器 / 老设备
        // 注意：只取前 4 位字符
        tag.insert_text(ItemKey::Year, d[..4].to_string());
    }

    if let Some(c) = copyright.as_ref().filter(|s| !s.is_empty()) {
        tag.insert_text(ItemKey::CopyrightMessage, c.clone());
    }

    // 5. 嵌入封面（前封面）
    if let Some(data) = cover_data {
        // lofty 0.25: unchecked(data) -> PictureBuilder -> build()
        let mut builder = Picture::unchecked(data.to_vec()).pic_type(PictureType::CoverFront);

        // mime 是可选的；有就设置，避免某些格式（如 Vorbis Comment）写入空 MIME
        if let Some(mime) = cover_mime {
            builder = builder.mime_type(mime);
        }

        tag.push_picture(builder.build());
    }
    
    if let Some(l) = lyric {
        if !l.is_empty() {
            tag.insert_text(ItemKey::Lyrics, l.to_owned());
        }
    }

    // 6. 落盘（lofty 会就地重写 ID3v2 / 追加 Vorbis Comment 块）
    tagged_file.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

fn detect_image_mime(data: &[u8]) -> Option<MimeType> {
    let kind = infer::get(data)?;
    if kind.matcher_type() != infer::MatcherType::Image {
        return None;
    }
    match kind.mime_type() {
        "image/png" => Some(MimeType::Png),
        "image/jpeg" => Some(MimeType::Jpeg),
        "image/gif" => Some(MimeType::Gif),
        "image/bmp" => Some(MimeType::Bmp),
        "image/tiff" => Some(MimeType::Tiff),
        _ => Some(MimeType::Unknown(kind.mime_type().to_owned())),   // 或者扩展 MimeType::Unknown
    }
}

pub fn make_album(info: AlbumInfo, downloaded: &AlbumDownloaded) {
    // COPY cover.EXT
    let cover_tmp_path = get_tmp_path(info.album.pic_url, &downloaded);
    let cover_bytes = fs::read(&cover_tmp_path).unwrap();
    let cover_ext = match infer::get(&cover_bytes){
        Some(x) => x.extension(),
        None => "jpg",
    };
    let cover_path = downloaded.paths.album_path.join(format!(
        "cover.{}",
        cover_ext
    ));
    fs::copy(&cover_tmp_path, &cover_path).unwrap();
    // COPY songs
    // ---- 专辑级元数据（只计算一次）----
    let album_name = info.album.name.clone();
    let album_artist = info
        .album
        .artists
        .iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join("; ");
    let copyright = if info.album.company.is_empty() {
        None
    } else {
        Some(info.album.company.clone())
    };
    let release_date: Option<String> = if info.album.publish_time > 0 {
        // UTC+8:00
        // Fuck you Timezone
        OffsetDateTime::from_unix_timestamp((info.album.publish_time / 1000) as i64)
            .ok()
            .map(|dt| {
                let local = dt + cookie::time::Duration::hours(8);
                format!(
                    "{:04}-{:02}-{:02}",
                    local.year(),
                    local.month() as u8,
                    local.day()
                )
            })
    } else {
        None
    };

    let total_track = info.songs.len();
    for x in info.songs {
        let safe_name = sanitize_filename_component(&x.d.name);
        let track_cover_path = get_tmp_path(x.d.al.pic_url, &downloaded);
        let cover_bytes = fs::read(&track_cover_path).ok();
        // Prepare Cover MIME
        let cover_mime = match &cover_bytes {
            Some(x) => detect_image_mime(&x),
            None => None,
        };
        //
        let track_tmp_path = get_tmp_path(x.d.url.unwrap().url, &downloaded);
        let disc_fmt;
        let track_fmt;
        let cd: DiscInfo = x.d.cd;
        if cd.total > 1 {
            disc_fmt = format!("{:0w$}.", cd.number, w = (cd.total.ilog10() + 1) as usize);
        } else {
            disc_fmt = String::new()
        }
        track_fmt = format!("{:0w$}", x.d.no, w = (total_track.ilog10() + 1) as usize);
        let filename = format!(
            "{}{}_{}.{}",
            disc_fmt,
            track_fmt,
            safe_name,
            track_tmp_path.extension().unwrap().to_string_lossy()
        );
        let song_path = downloaded.paths.album_path.join(&filename);
        eprintln!("WRITING {}", &filename);
        fs::copy(track_tmp_path, &song_path).unwrap();
        let artists: Vec<String> = x.d.ar.iter().map(|a| a.name.clone()).collect();
        let mix_lyric;
        if let Some(lyric_output) = x.d.lyric {
            mix_lyric = Some(lyric_output.mix);
            if !lyric_output.orig.is_empty() {
                let lrc_name = format!("{}{}_{}.lrc", disc_fmt, track_fmt, safe_name);
                eprintln!("WRITING {}", &lrc_name);
                let lrc_path = downloaded.paths.album_path.join(&lrc_name);
                if let Err(e) = fs::write(&lrc_path, lyric_output.orig) {
                    eprintln!("ORIG_LYRIC_WRITE_FAIL {}: {}", lrc_path.display(), e);
                }
            }
            if !lyric_output.trans.is_empty() {
                let lrc_name = format!("{}{}_{}.zh.lrc", disc_fmt, track_fmt, safe_name);
                eprintln!("WRITING {}", &lrc_name);
                let lrc_path = downloaded.paths.album_path.join(&lrc_name);
                if let Err(e) = fs::write(&lrc_path, lyric_output.trans) {
                    eprintln!("TRANS_LYRIC_WRITE_FAIL {}: {}", lrc_path.display(), e);
                }
            }
            if !lyric_output.roma.is_empty() {
                let lrc_name = format!("{}{}_{}.romaji.lrc", disc_fmt, track_fmt, safe_name);
                eprintln!("WRITING {}", &lrc_name);
                let lrc_path = downloaded.paths.album_path.join(&lrc_name);
                if let Err(e) = fs::write(&lrc_path, lyric_output.roma) {
                    eprintln!("ROMA_LYRIC_WRITE_FAIL {}: {}", lrc_path.display(), e);
                }
            }
        } else {
            mix_lyric = None
        }
        eprintln!("WRITING_META {}", &filename);
        if let Err(e) = write_audio_metadata(
            &song_path,
            &safe_name,
            &artists,
            &album_name,
            &album_artist,
            x.d.no as u32,
            cd.number,
            cd.total,
            total_track as u32,
            copyright.clone(),
            release_date.clone(), // ← 传完整日期
            cover_bytes.as_deref(),
            cover_mime,
            mix_lyric.as_deref(),
        ) {
            eprintln!("META_WRITE_FAIL {}: {}", song_path.display(), e);
        };
    }
}

pub fn cleanup(downloaded: &AlbumDownloaded){
    eprintln!("CLEANUP {}", downloaded.paths.tmp_path.clone().to_string_lossy());
    fs::remove_dir_all(downloaded.paths.tmp_path.clone()).unwrap();
    println!("{}", downloaded.paths.album_path.clone().to_string_lossy());
}
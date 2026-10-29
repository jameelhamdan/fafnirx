//! The subtitles of a video: the text tracks inside the file and the subtitle files next to it, offered as one list of
//! WebVTT tracks (`/files/hls/subtitles.json`, `/files/hls/subtitles/<n>.vtt`). Each track is made once, by `ffmpeg`, and kept with
//! the video's other cached things.

use crate::ffmpeg;
use crate::hls;
use crate::service::{fail, internal, locate, reply, App, Failure, Video, Which};
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;

/// Folders next to a video where subtitle files are kept, by their lower-case names.
const SUBTITLE_FOLDERS: [&str; 4] = ["subs", "sub", "subtitles", "subtitle"];

/// Extensions of the files that count as videos when deciding whether one is alone in its folder.
const VIDEO_EXTENSIONS: [&str; 10] = ["mkv", "mp4", "m4v", "avi", "mov", "webm", "ts", "wmv", "flv", "mpg"];

/// Most subtitle files looked at for one video.
const MAX_SUBTITLE_FILES: usize = 200;

/// Adds the subtitle files of `folder` to `files`, named `prefix` + the file name, and counts every video in it.
fn names_in(folder: &Path, prefix: &str, files: &mut Vec<String>, videos: &mut usize) {
    for e in std::fs::read_dir(folder).into_iter().flatten().flatten() {
        if !e.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let Ok(name) = e.file_name().into_string() else { continue };
        let Some((_, ext)) = name.rsplit_once('.') else { continue };
        let ext = ext.to_lowercase();
        if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
            *videos += 1;
        } else if hls::SUBTITLE_EXTENSIONS.contains(&ext.as_str()) && files.len() < MAX_SUBTITLE_FILES {
            files.push(format!("{prefix}{name}"));
        }
    }
}

/// The subtitle files that belong to the video: next to it (`Movie.srt`, `Movie.en.srt` for `Movie.mp4`) or in a `Subs`
/// folder next to it, which may hold a folder per video. A video that is alone in its folder gets every subtitle file there.
fn sidecar_subtitles(video: &Video) -> Vec<hls::Subtitle> {
    let Some(folder) = video.source.parent() else { return Vec::new() };
    let Some(name) = video.source.file_name().and_then(|n| n.to_str()) else { return Vec::new() };
    let (mut files, mut videos) = (Vec::new(), 0);
    names_in(folder, "", &mut files, &mut videos);
    for e in std::fs::read_dir(folder).into_iter().flatten().flatten() {
        let Ok(sub) = e.file_name().into_string() else { continue };
        if !e.file_type().is_ok_and(|t| t.is_dir()) || !SUBTITLE_FOLDERS.contains(&sub.to_lowercase().as_str()) {
            continue;
        }
        let mut ignored = 0;
        names_in(&e.path(), &format!("{sub}/"), &mut files, &mut ignored);
        for inner in std::fs::read_dir(e.path()).into_iter().flatten().flatten() {
            let Ok(dir) = inner.file_name().into_string() else { continue };
            if inner.file_type().is_ok_and(|t| t.is_dir()) {
                names_in(&inner.path(), &format!("{sub}/{dir}/"), &mut files, &mut ignored);
            }
        }
    }
    hls::subtitles_in(name, &files, videos == 1)
}

/// The subtitle tracks inside the video file, read once and kept with the video's other cached things. A file that
/// `ffprobe` cannot read has none (and is asked again next time).
async fn embedded_subtitles(app: &App, video: &Video) -> Vec<ffmpeg::Embedded> {
    let file = video.dir.join("embedded.json");
    let known =
        |file: &Path| std::fs::read(file).ok().and_then(|b| serde_json::from_slice::<Vec<ffmpeg::Embedded>>(&b).ok());
    if let Some(found) = known(&file) {
        return found;
    }
    let lock = app.lock_for(&file);
    let held = lock.lock().await;
    let found = match known(&file) {
        Some(found) => Some(found),
        None => {
            let _permit = app.jobs.acquire().await.expect("the semaphore is never closed");
            match ffmpeg::embedded_subtitles(&app.cfg.ffprobe, &video.source).await {
                Ok(found) => {
                    let _ = serde_json::to_vec(&found).map(|b| std::fs::write(&file, b));
                    Some(found)
                }
                Err(_) => None,
            }
        }
    };
    drop(held);
    app.done_with(&file);
    found.unwrap_or_default()
}

/// Every track a player can offer: the ones inside the video, then the files next to it.
async fn subtitles_of(app: &App, video: &Video) -> Vec<hls::Subtitle> {
    let mut tracks: Vec<hls::Subtitle> = embedded_subtitles(app, video)
        .await
        .into_iter()
        .map(|e| hls::embedded_subtitle(e.stream, &e.language, &e.title, e.forced))
        .collect();
    tracks.extend(sidecar_subtitles(video));
    hls::number_duplicates(&mut tracks);
    tracks
}

pub async fn list(
    State(app): State<Arc<App>>,
    Query(q): Query<Which>,
) -> Result<axum::Json<Vec<hls::Subtitle>>, Failure> {
    let video = locate(&app, &q)?;
    Ok(axum::Json(subtitles_of(&app, &video).await))
}

/// Subtitle files are text of a few hundred kilobytes at most.
const MAX_SUBTITLE_BYTES: u64 = 8 << 20;

/// One track as WebVTT, with times that are the video's own: the player shows it by the position in the video, so it
/// does not matter how the video is cut into segments. Made once per version of the file, then kept.
pub async fn track(
    State(app): State<Arc<App>>,
    UrlPath(file): UrlPath<String>,
    Query(q): Query<Which>,
) -> Result<Response, Failure> {
    let index: usize = file
        .strip_suffix(".vtt")
        .and_then(|n| {
            (!n.is_empty() && n.len() <= 4 && n.bytes().all(|b| b.is_ascii_digit())).then(|| n.parse().ok())?
        })
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "no such track"))?;
    let video = locate(&app, &q)?;
    let track = subtitles_of(&app, &video)
        .await
        .into_iter()
        .nth(index)
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "no such track"))?;
    if let Some(stream) = track.stream {
        return embedded_track(&app, &video, stream).await;
    }
    let path = video.source.parent().unwrap_or(&video.source).join(&track.file);
    let meta = std::fs::metadata(&path).map_err(|_| fail(StatusCode::NOT_FOUND, "no such track"))?;
    if meta.len() > MAX_SUBTITLE_BYTES {
        return Err(fail(StatusCode::PAYLOAD_TOO_LARGE, "that subtitle file is too large"));
    }
    // A changed file is a new track: the cache is named by what the file was.
    let mtime =
        meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let key = hex::encode(&Sha256::digest(format!("{}\0{}\0{mtime}", track.file, meta.len()).as_bytes())[..8]);
    let out = video.dir.join(format!("sub-{key}.vtt"));
    let (app_ref, language) = (&*app, &track.language);
    app.make_once(
        &video,
        "subtitles",
        &out,
        |tmp| async move { convert_subtitles(app_ref, &path, &tmp, language).await },
    )
    .await
    .map_err(|e| fail(StatusCode::UNSUPPORTED_MEDIA_TYPE, format!("that subtitle file cannot be read: {e:#}")))?;
    let bytes = std::fs::read(&out).map_err(|e| internal("reading the track", e.into()))?;
    Ok(reply("text/vtt; charset=utf-8", "private, max-age=3600", bytes))
}

/// A track inside the video as WebVTT, taken out with ffmpeg once per version of the video.
async fn embedded_track(app: &App, video: &Video, stream: usize) -> Result<Response, Failure> {
    let out = video.dir.join(format!("sub-s{stream}.vtt"));
    app.make_once(video, "subtitles", &out, |tmp| async move {
        let args = ffmpeg::embedded_subtitle_args(&video.source, stream, &tmp);
        ffmpeg::run(&app.cfg.ffmpeg, &args).await
    })
    .await
    .map_err(|e| fail(StatusCode::UNSUPPORTED_MEDIA_TYPE, format!("that subtitle track cannot be read: {e:#}")))?;
    let bytes = std::fs::read(&out).map_err(|e| internal("reading the track", e.into()))?;
    Ok(reply("text/vtt; charset=utf-8", "private, max-age=3600", bytes))
}

/// Writes `source` as WebVTT to `tmp`. A WebVTT file that is already UTF-8 is used as it is; anything else goes through
/// ffmpeg, in the character set it is written in: UTF-8 or UTF-16 as they are, an older file in the code page of its language.
async fn convert_subtitles(app: &App, source: &Path, tmp: &Path, language: &str) -> anyhow::Result<()> {
    let bytes = std::fs::read(source)?;
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&bytes);
    let is_vtt = source.extension().is_some_and(|e| e.eq_ignore_ascii_case("vtt"));
    let utf16 = bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]);
    let utf8 = std::str::from_utf8(body).is_ok();
    if is_vtt && utf8 && body.starts_with(b"WEBVTT") {
        std::fs::write(tmp, body)?;
    } else {
        let charset = (!utf8 && !utf16).then(|| hls::legacy_charset(language));
        ffmpeg::run(&app.cfg.ffmpeg, &ffmpeg::subtitle_args(source, tmp, charset)).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::sidecar_subtitles;

    #[test]
    fn subtitles_are_found_in_a_crowded_folder_and_in_subs() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..300 {
            std::fs::write(dir.path().join(format!("photo{i}.jpg")), b"").unwrap();
        }
        std::fs::write(dir.path().join("Show - 01.mkv"), b"").unwrap();
        std::fs::write(dir.path().join("Show - 01.eng.srt"), b"").unwrap();
        std::fs::create_dir(dir.path().join("Subs")).unwrap();
        std::fs::write(dir.path().join("Subs").join("Show - 01.jpn.ass"), b"").unwrap();
        let video = crate::service::Video {
            name: "Show - 01.mkv".into(),
            source: dir.path().join("Show - 01.mkv"),
            dir: dir.path().join("cache"),
        };
        let found: Vec<String> = sidecar_subtitles(&video).into_iter().map(|s| s.file).collect();
        assert_eq!(found, ["Show - 01.eng.srt", "Subs/Show - 01.jpn.ass"]);
    }
}

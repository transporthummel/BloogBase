//! Screenshots and videos the user took in DOSBox (PLAN.md §8.3). DOSBox
//! writes them to the profile's capture folder, which the launch config
//! points at.

use std::fs;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use specta::Type;

use crate::error::{Error, Result};
use crate::paths::AppPaths;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    /// File name in the capture folder.
    pub file: String,
    /// Absolute path, for the asset protocol.
    pub path: String,
    /// Unix epoch seconds.
    pub taken_at: Option<u32>,
    pub size: u32,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    /// File name in the capture folder.
    pub file: String,
    /// Unix epoch seconds.
    pub taken_at: Option<u32>,
    pub size: u32,
    /// Seconds; unknown while DOSBox is still recording.
    pub duration: Option<u32>,
    /// PNG of the first frame, for the asset protocol.
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Screenshot,
    Video,
}

impl Kind {
    fn matches(self, name: &str) -> bool {
        let ext = match self {
            Kind::Screenshot => ".png",
            Kind::Video => ".avi",
        };
        name.to_ascii_lowercase().ends_with(ext)
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Screenshot => "screenshot",
            Kind::Video => "video",
        }
    }
}

struct Entry {
    file: String,
    path: PathBuf,
    modified: Option<SystemTime>,
    size: u32,
}

impl Entry {
    fn taken_at(&self) -> Option<u32> {
        self.modified
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as u32)
    }
}

/// The profile's captures of one kind, newest first.
fn entries(paths: &AppPaths, profile_id: &str, kind: Kind) -> Result<Vec<Entry>> {
    let dir = paths.profile(profile_id).capture;
    let read = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut entries = Vec::new();
    for entry in read {
        let entry = entry?;
        let meta = entry.metadata()?;
        let Ok(file) = entry.file_name().into_string() else {
            continue;
        };
        if !meta.is_file() || !kind.matches(&file) {
            continue;
        }
        entries.push(Entry {
            path: entry.path(),
            file,
            modified: meta.modified().ok(),
            size: meta.len().min(u32::MAX as u64) as u32,
        });
    }
    entries.sort_by(|a, b| b.modified.cmp(&a.modified).then(b.file.cmp(&a.file)));
    Ok(entries)
}

pub fn screenshots(paths: &AppPaths, profile_id: &str) -> Result<Vec<Screenshot>> {
    Ok(entries(paths, profile_id, Kind::Screenshot)?
        .into_iter()
        .map(|e| Screenshot {
            taken_at: e.taken_at(),
            path: e.path.display().to_string(),
            file: e.file,
            size: e.size,
        })
        .collect())
}

pub fn videos(paths: &AppPaths, profile_id: &str) -> Result<Vec<Video>> {
    let thumbs = paths.profile(profile_id).video_thumbs;
    Ok(entries(paths, profile_id, Kind::Video)?
        .into_iter()
        .map(|e| {
            let (duration, thumbnail) = match video_info(&e, &thumbs) {
                Ok(info) => info,
                Err(err) => {
                    tracing::debug!("can't read video {}: {err}", e.path.display());
                    (None, None)
                }
            };
            Video {
                taken_at: e.taken_at(),
                file: e.file,
                size: e.size,
                duration,
                thumbnail: thumbnail.map(|p| p.display().to_string()),
            }
        })
        .collect())
}

/// A capture's path; refuses names that would leave the capture folder.
pub fn path(paths: &AppPaths, profile_id: &str, kind: Kind, file: &str) -> Result<PathBuf> {
    let not_found = || Error::NotFound(format!("{} {file}", kind.name()));
    if file.is_empty() || file.contains(['/', '\\']) || file == ".." || !kind.matches(file) {
        return Err(not_found());
    }
    let path = paths.profile(profile_id).capture.join(file);
    if !path.is_file() {
        return Err(not_found());
    }
    Ok(path)
}

pub fn delete(paths: &AppPaths, profile_id: &str, kind: Kind, file: &str) -> Result<()> {
    fs::remove_file(path(paths, profile_id, kind, file)?)?;
    if kind == Kind::Video {
        let thumb = thumbnail_path(&paths.profile(profile_id).video_thumbs, file);
        if let Err(e) = fs::remove_file(&thumb)
            && e.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!("could not remove {}: {e}", thumb.display());
        }
    }
    tracing::info!("deleted {} {file} of {profile_id}", kind.name());
    Ok(())
}

fn thumbnail_path(thumbs: &Path, file: &str) -> PathBuf {
    thumbs.join(format!("{file}.png"))
}

/// The video's length and its first frame as PNG, made once per recording.
fn video_info(video: &Entry, thumbs: &Path) -> io::Result<(Option<u32>, Option<PathBuf>)> {
    let avi = read_avi(&video.path)?;
    let thumb = thumbnail_path(thumbs, &video.file);
    let made = fs::metadata(&thumb).and_then(|m| m.modified()).ok();
    if made.is_some() && made >= video.modified {
        return Ok((avi.duration, Some(thumb)));
    }
    let Some(rgb) = avi
        .first_frame
        .as_deref()
        .and_then(|f| zmbv_keyframe(f, avi.width, avi.height))
    else {
        return Ok((avi.duration, None));
    };
    fs::create_dir_all(thumbs)?;
    crate::covers::write_rgb_png(avi.width as u32, avi.height as u32, &rgb, &thumb)
        .map_err(io::Error::other)?;
    Ok((avi.duration, Some(thumb)))
}

/// What DOSBox's AVI files reveal without decoding them all.
struct Avi {
    duration: Option<u32>,
    width: usize,
    height: usize,
    /// The first video frame, as stored.
    first_frame: Option<Vec<u8>>,
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

fn read_avi(path: &Path) -> io::Result<Avi> {
    let mut r = BufReader::new(fs::File::open(path)?);
    let mut word = [0u8; 4];
    let mut header = |r: &mut BufReader<fs::File>| -> io::Result<([u8; 4], u32)> {
        r.read_exact(&mut word)?;
        let id = word;
        r.read_exact(&mut word)?;
        Ok((id, u32::from_le_bytes(word)))
    };
    let (riff, _) = header(&mut r)?;
    let mut form = [0u8; 4];
    r.read_exact(&mut form)?;
    if &riff != b"RIFF" || &form != b"AVI " {
        return Err(invalid("not an AVI file"));
    }

    let mut avi = Avi {
        duration: None,
        width: 0,
        height: 0,
        first_frame: None,
    };
    loop {
        let (id, size) = match header(&mut r) {
            Ok(h) => h,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };
        let start = r.stream_position()?;
        // DOSBox fills in sizes when it stops recording.
        let end = (size > 0).then(|| start + u64::from(size) + u64::from(size & 1));
        if &id == b"LIST" {
            r.read_exact(&mut form)?;
            if &form == b"hdrl" {
                let (id, size) = header(&mut r)?;
                if &id == b"avih" && size >= 40 {
                    let mut avih = [0u8; 40];
                    r.read_exact(&mut avih)?;
                    let field =
                        |at: usize| u32::from_le_bytes(avih[at..at + 4].try_into().unwrap());
                    let frame_us = u64::from(field(0));
                    let frames = u64::from(field(16));
                    avi.width = field(32) as usize;
                    avi.height = field(36) as usize;
                    avi.duration =
                        (frames > 0).then(|| ((frames * frame_us + 500_000) / 1_000_000) as u32);
                }
            } else if &form == b"movi" {
                avi.first_frame = first_frame(&mut r, end)?;
                break;
            }
        }
        match end {
            Some(end) => r.seek(SeekFrom::Start(end))?,
            None => break,
        };
    }
    Ok(avi)
}

/// The first non-empty video chunk in the `movi` list.
fn first_frame(r: &mut BufReader<fs::File>, end: Option<u64>) -> io::Result<Option<Vec<u8>>> {
    /// Far above any frame DOSBox writes; guards against broken sizes.
    const MAX_FRAME: u32 = 64 << 20;
    let mut head = [0u8; 8];
    while end.is_none_or(|end| r.stream_position().is_ok_and(|p| p + 8 <= end)) {
        match r.read_exact(&mut head) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
        let size = u32::from_le_bytes(head[4..].try_into().unwrap());
        match &head[..4] {
            // A `rec ` list just groups chunks; step into it.
            b"LIST" => {
                r.seek_relative(4)?;
            }
            [_, _, b'd', b'b' | b'c'] if size > 0 && size <= MAX_FRAME => {
                let mut frame = vec![0; size as usize];
                r.read_exact(&mut frame)?;
                return Ok(Some(frame));
            }
            _ => {
                r.seek_relative(i64::from(size) + i64::from(size & 1))?;
            }
        }
    }
    Ok(None)
}

/// Decodes a ZMBV key frame (DOSBox's video codec) to RGB. `None` for delta
/// frames and formats DOSBox doesn't write.
fn zmbv_keyframe(frame: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    use flate2::{Decompress, FlushDecompress};

    let (head, body) = frame.split_at_checked(7)?;
    let &[flags, _, _, compression, format, ..] = head else {
        return None;
    };
    if flags & 1 == 0 || width == 0 || height == 0 {
        return None;
    }
    let pixels = width.checked_mul(height)?;
    let len = match format {
        // 8 bpp, after a 256 color RGB palette.
        4 => 768 + pixels,
        // 15 and 16 bpp
        5 | 6 => pixels * 2,
        // 32 bpp
        8 => pixels * 4,
        _ => return None,
    };
    let raw = match compression {
        0 => body.to_vec(),
        1 => {
            let mut out = Vec::with_capacity(len);
            Decompress::new(true)
                .decompress_vec(body, &mut out, FlushDecompress::Sync)
                .ok()?;
            out
        }
        _ => return None,
    };
    let raw = raw.get(..len)?;

    let mut rgb = Vec::with_capacity(pixels * 3);
    let five = |c: u16| ((c << 3) | (c >> 2)) as u8;
    match format {
        4 => {
            let (palette, indices) = raw.split_at(768);
            for &i in indices {
                rgb.extend_from_slice(&palette[i as usize * 3..][..3]);
            }
        }
        5 => {
            for &p in raw.as_chunks::<2>().0 {
                let v = u16::from_le_bytes(p);
                rgb.extend([five((v >> 10) & 31), five((v >> 5) & 31), five(v & 31)]);
            }
        }
        6 => {
            for &p in raw.as_chunks::<2>().0 {
                let v = u16::from_le_bytes(p);
                let g = (v >> 5) & 63;
                rgb.extend([five(v >> 11), ((g << 2) | (g >> 4)) as u8, five(v & 31)]);
            }
        }
        _ => {
            for &[b, g, r, _] in raw.as_chunks::<4>().0 {
                rgb.extend([r, g, b]);
            }
        }
    }
    Some(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn touch(path: &Path, age: u64) {
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - std::time::Duration::from_secs(age))
            .unwrap();
    }

    #[test]
    fn lists_pngs_newest_first_and_deletes_safely() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path());
        assert!(screenshots(&paths, "keen4-1").unwrap().is_empty());

        let dir = paths.profile("keen4-1").capture;
        fs::create_dir_all(dir.join("sub")).unwrap();
        for (name, age) in [
            ("image0001.png", 20),
            ("IMAGE0002.PNG", 10),
            ("audio0001.wav", 0),
        ] {
            fs::File::create(dir.join(name)).unwrap();
            touch(&dir.join(name), age);
        }
        fs::write(dir.join("sub").join("image0003.png"), b"").unwrap();

        let shots = screenshots(&paths, "keen4-1").unwrap();
        let names: Vec<_> = shots.iter().map(|s| s.file.as_str()).collect();
        assert_eq!(names, ["IMAGE0002.PNG", "image0001.png"]);
        assert!(PathBuf::from(&shots[0].path).is_absolute());

        for bad in [
            "",
            "..",
            "sub/image0003.png",
            "..\\x.png",
            "audio0001.wav",
            "gone.png",
        ] {
            assert!(
                matches!(
                    delete(&paths, "keen4-1", Kind::Screenshot, bad),
                    Err(Error::NotFound(_))
                ),
                "{bad}"
            );
        }
        delete(&paths, "keen4-1", Kind::Screenshot, "image0001.png").unwrap();
        assert_eq!(screenshots(&paths, "keen4-1").unwrap().len(), 1);
    }

    fn chunk(id: &[u8], body: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        if body.len() % 2 == 1 {
            out.push(0);
        }
        out
    }

    fn list(kind: &[u8], chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut body = kind.to_vec();
        body.extend(chunks.concat());
        chunk(b"LIST", &body)
    }

    /// A 2×1 AVI as DOSBox writes it: 8 bpp ZMBV, 70 frames at 70 Hz.
    fn avi() -> Vec<u8> {
        let mut avih = vec![0u8; 56];
        avih[0..4].copy_from_slice(&14_286u32.to_le_bytes());
        avih[16..20].copy_from_slice(&70u32.to_le_bytes());
        avih[32..36].copy_from_slice(&2u32.to_le_bytes());
        avih[36..40].copy_from_slice(&1u32.to_le_bytes());
        let mut pixels = vec![0u8; 768];
        pixels[3..6].copy_from_slice(&[0, 0, 0xAA]);
        pixels[6..9].copy_from_slice(&[0xFF, 0x55, 0x55]);
        pixels.extend([1, 2]);
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&pixels).unwrap();
        let mut frame = vec![1, 0, 1, 1, 4, 16, 16];
        frame.extend(z.finish().unwrap());

        let mut body = b"AVI ".to_vec();
        body.extend(list(b"hdrl", &[chunk(b"avih", &avih)]));
        body.extend(chunk(b"JUNK", &[0; 3]));
        body.extend(list(
            b"movi",
            &[
                chunk(b"01wb", &[0; 8]),
                chunk(b"00dc", &[]),
                chunk(b"00dc", &frame),
            ],
        ));
        chunk(b"RIFF", &body)
    }

    #[test]
    fn lists_videos_with_length_and_first_frame() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path());
        let dir = paths.profile("keen4-1").capture;
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("video0001.avi"), avi()).unwrap();
        touch(&dir.join("video0001.avi"), 10);
        // Still recording: no frames yet.
        fs::write(dir.join("video0002.avi"), &avi()[..120]).unwrap();
        fs::write(dir.join("image0001.png"), b"").unwrap();

        let videos = videos(&paths, "keen4-1").unwrap();
        let names: Vec<_> = videos.iter().map(|v| v.file.as_str()).collect();
        assert_eq!(names, ["video0002.avi", "video0001.avi"]);
        assert_eq!(videos[0].thumbnail, None);
        assert_eq!(videos[1].duration, Some(1));

        let thumb = PathBuf::from(videos[1].thumbnail.as_ref().unwrap());
        let decoder = png::Decoder::new(io::BufReader::new(fs::File::open(&thumb).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut rgb = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut rgb).unwrap();
        assert_eq!(rgb, [0, 0, 0xAA, 0xFF, 0x55, 0x55]);

        delete(&paths, "keen4-1", Kind::Video, "video0001.avi").unwrap();
        assert!(!thumb.exists());
        assert!(matches!(
            delete(&paths, "keen4-1", Kind::Video, "image0001.png"),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn decodes_zmbv_formats() {
        // 15 bpp white, 16 bpp pure green, 32 bpp BGRX.
        assert_eq!(
            zmbv_keyframe(&[1, 0, 1, 0, 5, 16, 16, 0xFF, 0x7F], 1, 1),
            Some(vec![255; 3])
        );
        assert_eq!(
            zmbv_keyframe(&[1, 0, 1, 0, 6, 16, 16, 0xE0, 0x07], 1, 1),
            Some(vec![0, 255, 0])
        );
        assert_eq!(
            zmbv_keyframe(&[1, 0, 1, 0, 8, 16, 16, 1, 2, 3, 0], 1, 1),
            Some(vec![3, 2, 1])
        );
        // Delta frames and short data.
        assert_eq!(
            zmbv_keyframe(&[0, 0, 1, 0, 8, 16, 16, 1, 2, 3, 0], 1, 1),
            None
        );
        assert_eq!(zmbv_keyframe(&[1, 0, 1, 0, 8, 16, 16, 1, 2], 1, 1), None);
    }
}

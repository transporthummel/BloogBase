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
    /// Absolute path, for the asset protocol.
    pub path: String,
    /// Unix epoch seconds.
    pub taken_at: Option<u32>,
    pub size: u32,
    /// Seconds; unknown while DOSBox is still recording.
    pub duration: Option<u32>,
    /// PNG of the first frame, for the asset protocol.
    pub thumbnail: Option<String>,
}

/// Where a video's frames and sound are, for playing it in the app.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoIndex {
    pub width: u32,
    pub height: u32,
    /// Microseconds per frame.
    pub frame_us: u32,
    /// Byte offset and size of every frame; an empty one repeats the last.
    pub frames: Vec<[u32; 2]>,
    /// Numbers of the frames that don't build on earlier ones.
    pub key_frames: Vec<u32>,
    /// Only 16-bit PCM, which is what DOSBox records.
    pub audio: Option<VideoAudio>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VideoAudio {
    pub rate: u32,
    pub channels: u32,
    /// Byte offset and size of every chunk of samples.
    pub chunks: Vec<[u32; 2]>,
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
                path: e.path.display().to_string(),
                file: e.file,
                size: e.size,
                duration,
                thumbnail: thumbnail.map(|p| p.display().to_string()),
            }
        })
        .collect())
}

/// A capture's path; refuses names that would leave the capture folder.
fn path(paths: &AppPaths, profile_id: &str, kind: Kind, file: &str) -> Result<PathBuf> {
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

pub fn video_index(paths: &AppPaths, profile_id: &str, file: &str) -> Result<VideoIndex> {
    let avi = read_avi(&path(paths, profile_id, Kind::Video, file)?, true)?;
    // AVI files stay below 4 GiB; anything past that is broken.
    let fits = |offset: u64, size: u32| u32::try_from(offset).ok().map(|o| [o, size]);
    let frames: Vec<_> = avi
        .frames
        .iter()
        .map_while(|&(o, s, _)| fits(o, s))
        .collect();
    let key_frames = (0..frames.len() as u32)
        .filter(|&i| avi.frames[i as usize].2)
        .collect();
    let audio = avi
        .audio
        .filter(|a| a.tag == 1 && a.bits == 16 && a.channels > 0)
        .map(|a| VideoAudio {
            rate: a.rate,
            channels: u32::from(a.channels),
            chunks: avi
                .audio_chunks
                .iter()
                .map_while(|&(o, s)| fits(o, s))
                .collect(),
        });
    Ok(VideoIndex {
        width: avi.width as u32,
        height: avi.height as u32,
        frame_us: avi.frame_us,
        frames,
        key_frames,
        audio,
    })
}

fn thumbnail_path(thumbs: &Path, file: &str) -> PathBuf {
    thumbs.join(format!("{file}.png"))
}

/// The video's length and its first frame as PNG, made once per recording.
fn video_info(video: &Entry, thumbs: &Path) -> io::Result<(Option<u32>, Option<PathBuf>)> {
    let avi = read_avi(&video.path, false)?;
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

/// What DOSBox's AVI files hold, read without decoding any frames.
struct Avi {
    duration: Option<u32>,
    width: usize,
    height: usize,
    frame_us: u32,
    audio: Option<AudioFormat>,
    /// Offset and size of every video frame, and whether it's a key frame.
    frames: Vec<(u64, u32, bool)>,
    /// Offset and size of every audio chunk.
    audio_chunks: Vec<(u64, u32)>,
    /// The first non-empty frame, if only that was asked for.
    first_frame: Option<Vec<u8>>,
}

#[derive(Clone, Copy)]
struct AudioFormat {
    tag: u16,
    channels: u16,
    rate: u32,
    bits: u16,
}

#[derive(Clone, Copy, PartialEq)]
enum Stream {
    Video,
    Audio,
    Other,
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

/// Reads the headers and, with `full`, where every chunk is; otherwise
/// stops at the first frame.
fn read_avi(path: &Path, full: bool) -> io::Result<Avi> {
    let mut r = BufReader::new(fs::File::open(path)?);
    let mut head = [0u8; 12];
    r.read_exact(&mut head)?;
    if &head[..4] != b"RIFF" || &head[8..] != b"AVI " {
        return Err(invalid("not an AVI file"));
    }
    let mut avi = Avi {
        duration: None,
        width: 0,
        height: 0,
        frame_us: 0,
        audio: None,
        frames: Vec::new(),
        audio_chunks: Vec::new(),
        first_frame: None,
    };
    walk(&mut r, None, &mut avi, &mut Vec::new(), full)?;
    Ok(avi)
}

/// Goes through the chunks up to `end` (the file's end if unknown); `true`
/// once there's nothing more to read.
fn walk(
    r: &mut BufReader<fs::File>,
    end: Option<u64>,
    avi: &mut Avi,
    streams: &mut Vec<Stream>,
    full: bool,
) -> io::Result<bool> {
    /// Far above any frame DOSBox writes; guards against broken sizes.
    const MAX_FRAME: u32 = 64 << 20;
    let mut head = [0u8; 8];
    while end.is_none_or(|end| r.stream_position().is_ok_and(|p| p + 8 <= end)) {
        match r.read_exact(&mut head) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(true),
            Err(e) => return Err(e),
        }
        let id: [u8; 4] = head[..4].try_into().unwrap();
        let size = u32::from_le_bytes(head[4..].try_into().unwrap());
        let start = r.stream_position()?;
        let chunk_end = start + u64::from(size) + u64::from(size & 1);
        match &id {
            b"LIST" => {
                let mut form = [0u8; 4];
                r.read_exact(&mut form)?;
                if matches!(&form, b"hdrl" | b"strl" | b"movi" | b"rec ") {
                    // DOSBox fills in list sizes when it stops recording.
                    let list_end = (size > 0).then_some(chunk_end);
                    if walk(r, list_end, avi, streams, full)? || list_end.is_none() {
                        return Ok(true);
                    }
                }
            }
            b"avih" if size >= 40 => {
                let mut avih = [0u8; 40];
                r.read_exact(&mut avih)?;
                let field = |at: usize| u32::from_le_bytes(avih[at..at + 4].try_into().unwrap());
                avi.frame_us = field(0);
                let frames = u64::from(field(16));
                avi.width = field(32) as usize;
                avi.height = field(36) as usize;
                avi.duration = (frames > 0)
                    .then(|| ((frames * u64::from(avi.frame_us) + 500_000) / 1_000_000) as u32);
            }
            b"strh" if size >= 4 => {
                let mut kind = [0u8; 4];
                r.read_exact(&mut kind)?;
                streams.push(match &kind {
                    b"vids" => Stream::Video,
                    b"auds" => Stream::Audio,
                    _ => Stream::Other,
                });
            }
            b"strf" if size >= 16 && streams.last() == Some(&Stream::Audio) => {
                let mut wave = [0u8; 16];
                r.read_exact(&mut wave)?;
                let half = |at: usize| u16::from_le_bytes([wave[at], wave[at + 1]]);
                avi.audio = Some(AudioFormat {
                    tag: half(0),
                    channels: half(2),
                    rate: u32::from_le_bytes(wave[4..8].try_into().unwrap()),
                    bits: half(14),
                });
            }
            [a, b, kind @ ..] if a.is_ascii_digit() && b.is_ascii_digit() => {
                let stream = streams.get(usize::from((a - b'0') * 10 + (b - b'0')));
                match (stream, kind) {
                    (Some(Stream::Video), b"dc" | b"db") if !full => {
                        if size > 0 && size <= MAX_FRAME {
                            let mut frame = vec![0; size as usize];
                            r.read_exact(&mut frame)?;
                            avi.first_frame = Some(frame);
                            return Ok(true);
                        }
                    }
                    (Some(Stream::Video), b"dc" | b"db") => {
                        let mut flags = [0u8];
                        let key = size > 0 && r.read_exact(&mut flags).is_ok() && flags[0] & 1 == 1;
                        avi.frames.push((start, size, key));
                    }
                    (Some(Stream::Audio), b"wb") if full => avi.audio_chunks.push((start, size)),
                    _ => {}
                }
            }
            _ => {}
        }
        r.seek(SeekFrom::Start(chunk_end))?;
    }
    Ok(false)
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

        let mut wave = vec![1, 0, 2, 0];
        wave.extend(48_000u32.to_le_bytes());
        wave.extend(192_000u32.to_le_bytes());
        wave.extend([4, 0, 16, 0]);

        let mut body = b"AVI ".to_vec();
        body.extend(list(
            b"hdrl",
            &[
                chunk(b"avih", &avih),
                list(
                    b"strl",
                    &[chunk(b"strh", b"vidsZMBV"), chunk(b"strf", &[0; 40])],
                ),
                list(b"strl", &[chunk(b"strh", b"auds"), chunk(b"strf", &wave)]),
            ],
        ));
        body.extend(chunk(b"JUNK", &[0; 3]));
        body.extend(list(
            b"movi",
            &[
                chunk(b"01wb", &[0; 8]),
                chunk(b"00dc", &[]),
                chunk(b"00dc", &frame),
                chunk(b"01wb", &[0; 4]),
                chunk(b"00dc", &[0, 1, 2]),
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
    fn indexes_frames_and_sound() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::new(tmp.path());
        let dir = paths.profile("keen4-1").capture;
        fs::create_dir_all(&dir).unwrap();
        let data = avi();
        fs::write(dir.join("video0001.avi"), &data).unwrap();

        let index = video_index(&paths, "keen4-1", "video0001.avi").unwrap();
        assert_eq!((index.width, index.height, index.frame_us), (2, 1, 14_286));
        let at = |[offset, size]: [u32; 2]| &data[offset as usize..][..size as usize];
        let sizes: Vec<_> = index.frames.iter().map(|f| f[1]).collect();
        assert_eq!(sizes[0], 0);
        assert_eq!(at(index.frames[1])[..5], [1, 0, 1, 1, 4]);
        assert_eq!(at(index.frames[2]), [0, 1, 2]);
        assert_eq!(index.key_frames, [1]);
        let audio = index.audio.unwrap();
        assert_eq!((audio.rate, audio.channels), (48_000, 2));
        let sizes: Vec<_> = audio.chunks.iter().map(|c| c[1]).collect();
        assert_eq!(sizes, [8, 4]);

        assert!(matches!(
            video_index(&paths, "keen4-1", "../video0001.avi"),
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

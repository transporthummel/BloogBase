import { convertFileSrc } from "@tauri-apps/api/core";
import { Film } from "lucide-react";
import { commands, type Profile, type Video } from "../bindings";
import { unwrap } from "../ipc";
import { CapturesPanel } from "./CapturesPanel";
import { formatDuration, useVideos, videosKey } from "./captures";
import { VideoPlayer } from "./VideoPlayer";

/** DOSBox Staging's video recording hotkey. */
function hotkey(): string {
  return /Mac/.test(navigator.userAgent) ? "⌘ + F7" : "Ctrl + F7";
}

/** The first frame, stretched to 4:3 like DOSBox shows it. */
function Poster({ video }: { video: Video }) {
  return video.thumbnail ? (
    <img
      src={convertFileSrc(video.thumbnail)}
      alt=""
      loading="lazy"
      className="aspect-[4/3] w-full object-fill [image-rendering:pixelated]"
    />
  ) : (
    <div className="grid aspect-[4/3] w-full place-items-center text-muted">
      <Film className="size-10" aria-hidden />
    </div>
  );
}

/** Videos the player recorded in DOSBox, played in the app (§8.3). */
export function VideosPanel({ profile }: { profile: Profile }) {
  return (
    <CapturesPanel
      profile={profile}
      kind="videos"
      hotkey={hotkey()}
      captures={useVideos(profile.id)}
      queryKey={videosKey(profile.id)}
      remove={(file) => unwrap(commands.deleteVideo(profile.id, file))}
      tile={(video) => (
        <>
          <Poster video={video} />
          {video.duration !== null && (
            <span className="absolute right-1.5 bottom-1.5 rounded bg-black/75 px-1.5 py-0.5 text-xs text-white tabular-nums">
              {formatDuration(video.duration)}
            </span>
          )}
        </>
      )}
      viewer={(video) => <VideoPlayer key={video.file} profileId={profile.id} video={video} />}
    />
  );
}

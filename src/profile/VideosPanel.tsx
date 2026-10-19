import { useMutation } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Film, Play } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, type Profile, type Video } from "../bindings";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { CapturesPanel } from "./CapturesPanel";
import { formatDuration, useVideos, videosKey } from "./captures";

/** DOSBox Staging's video recording hotkey. */
function hotkey(): string {
  return /Mac/.test(navigator.userAgent) ? "⌘ + F7" : "Ctrl + F7";
}

/**
 * The first frame, stretched to 4:3 like DOSBox shows it. The webview can't
 * decode DOSBox's codec, so videos play in the system's player.
 */
function Poster({ video, className = "" }: { video: Video; className?: string }) {
  return video.thumbnail ? (
    <img
      src={convertFileSrc(video.thumbnail)}
      alt=""
      loading="lazy"
      className={`aspect-[4/3] w-full object-fill [image-rendering:pixelated] ${className}`}
    />
  ) : (
    <div className={`grid aspect-[4/3] w-full place-items-center text-muted ${className}`}>
      <Film className="size-10" aria-hidden />
    </div>
  );
}

/** Videos the player recorded in DOSBox (§8.3). */
export function VideosPanel({ profile }: { profile: Profile }) {
  const { t } = useTranslation();
  const play = useMutation({
    mutationFn: (file: string) => unwrap(commands.openVideo(profile.id, file)),
  });

  return (
    <CapturesPanel
      profile={profile}
      kind="videos"
      hotkey={hotkey()}
      captures={useVideos(profile.id)}
      queryKey={videosKey(profile.id)}
      remove={(file) => unwrap(commands.deleteVideo(profile.id, file))}
      error={play.error}
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
      viewer={(video) => (
        <div className="grid gap-2">
          <Poster video={video} className="mx-auto max-w-[80vh] rounded bg-black" />
          <p className="text-sm text-muted">{t("videos.player")}</p>
        </div>
      )}
      actions={(video) => (
        <Button onClick={() => play.mutate(video.file)}>
          <Play className="size-4" aria-hidden />
          {t("videos.play")}
        </Button>
      )}
    />
  );
}

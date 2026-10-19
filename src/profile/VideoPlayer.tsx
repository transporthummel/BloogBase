import { useQuery } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Pause, Play } from "lucide-react";
import { useEffect, useReducer, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Video } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";
import { formatDuration } from "./captures";
import { VideoPlayback } from "./videoPlayback";

/** Plays a DOSBox video in the app, stretched to 4:3 like DOSBox shows it. */
export function VideoPlayer({ profileId, video }: { profileId: string; video: Video }) {
  const { t } = useTranslation();
  const index = useQuery({
    // A video still being recorded grows.
    queryKey: ["videoIndex", profileId, video.file, video.size],
    queryFn: () => unwrap(commands.videoIndex(profileId, video.file)),
  });
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [player, setPlayer] = useState<VideoPlayback | null>(null);
  const [, update] = useReducer((n: number) => n + 1, 0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!index.data || !canvas || index.data.frames.length === 0) return;
    const playback = new VideoPlayback(convertFileSrc(video.path), index.data, canvas, update);
    setPlayer(playback);
    playback.play();
    return () => {
      playback.destroy();
      setPlayer(null);
    };
  }, [index.data, video.path]);

  const error = index.error ?? player?.error;
  const toggle = () => (player?.paused ? player.play() : player?.pause());
  const time = player?.time ?? 0;
  const duration = player?.duration ?? 0;

  return (
    <div className="grid gap-2">
      <canvas
        ref={canvasRef}
        onClick={toggle}
        aria-label={t("videos.alt")}
        className="mx-auto aspect-[4/3] w-full max-w-[80vh] rounded bg-black [image-rendering:pixelated]"
      />
      {!!error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
      <div className="flex items-center gap-3">
        <Button
          variant="secondary"
          onClick={toggle}
          disabled={!player || !!error}
          aria-label={player && !player.paused ? t("videos.pause") : t("videos.play")}
        >
          {player && !player.paused ? (
            <Pause className="size-4" aria-hidden />
          ) : (
            <Play className="size-4" aria-hidden />
          )}
        </Button>
        <input
          type="range"
          min={0}
          max={duration}
          step="any"
          value={time}
          onChange={(e) => player?.seek(Number(e.target.value))}
          disabled={!player}
          aria-label={t("videos.position")}
          className="min-w-0 flex-1 accent-accent"
        />
        <span className="text-sm text-muted tabular-nums">
          {formatDuration(Math.floor(time))} / {formatDuration(Math.round(duration))}
        </span>
      </div>
    </div>
  );
}

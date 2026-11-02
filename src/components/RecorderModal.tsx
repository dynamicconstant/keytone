import { useEffect, useRef, useState } from "react";
import { api, errorText } from "../lib/api";
import type { Toasts } from "./Toast";

type Phase = "idle" | "recording" | "review" | "saving";

export default function RecorderModal({
  onClose,
  onSaved,
  toasts,
}: {
  onClose: () => void;
  onSaved: (file: string) => void;
  toasts: Toasts;
}) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [level, setLevel] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [maxSecs, setMaxSecs] = useState(10);
  const [length, setLength] = useState(0);
  const [name, setName] = useState("my-sound");
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.clearInterval(timer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const close = () => {
    window.clearInterval(timer.current);
    api.discardTake().catch(() => {});
    onClose();
  };

  const start = async () => {
    try {
      await api.startRecording();
      setPhase("recording");
      timer.current = window.setInterval(async () => {
        const s = await api.recordingStatus();
        setLevel((l) => Math.max(s.level, l * 0.85));
        setElapsed(s.elapsed);
        setMaxSecs(s.max_secs);
        if (s.elapsed >= s.max_secs - 0.05) stop();
      }, 60);
    } catch (e) {
      toasts.error(errorText(e));
    }
  };

  const stop = async () => {
    window.clearInterval(timer.current);
    try {
      const secs = await api.stopRecording();
      setLength(secs);
      setPhase("review");
    } catch (e) {
      toasts.error(errorText(e));
      setPhase("idle");
    }
    setLevel(0);
  };

  const save = async () => {
    setPhase("saving");
    try {
      const file = await api.saveTake(name);
      onSaved(file);
    } catch (e) {
      toasts.error(errorText(e));
      setPhase("review");
    }
  };

  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal" role="dialog" aria-label="Record a sound" onClick={(e) => e.stopPropagation()}>
        <h2>Record a sound</h2>
        <p className="muted">
          Make the sound close to your mic — a pen click, a tap on the desk, your voice. Silence before and after is trimmed automatically.
        </p>

        <div className="meter">
          <div className="meter-fill" style={{ width: `${Math.min(100, level * 140)}%` }} />
        </div>
        <div className="muted small">
          {phase === "recording" ? `${elapsed.toFixed(1)} s / ${maxSecs} s` : phase === "review" ? `Recorded ${length.toFixed(2)} s` : "Ready"}
        </div>

        <div className="modal-actions">
          {phase === "idle" && (
            <button className="btn primary rec" onClick={start}>
              ● Record
            </button>
          )}
          {phase === "recording" && (
            <button className="btn primary" onClick={stop}>
              ■ Stop
            </button>
          )}
          {(phase === "review" || phase === "saving") && (
            <>
              <button className="btn" onClick={() => api.previewTake()}>
                ▶ Play
              </button>
              <button className="btn" onClick={() => setPhase("idle")}>
                Retake
              </button>
              <label className="field inline">
                <span>Name</span>
                <input value={name} onChange={(e) => setName(e.target.value)} />
              </label>
              <button className="btn primary" disabled={phase === "saving"} onClick={save}>
                Save & assign
              </button>
            </>
          )}
          <span className="spacer" />
          <button className="btn ghost" onClick={close}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

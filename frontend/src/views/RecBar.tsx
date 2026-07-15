import { createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

// linux recording control bar: elapsed clock + stop. the window is created
// when the recording starts, so the clock counts from mount.
export function RecBar() {
  const [elapsed, setElapsed] = createSignal("00:00");
  const started = Date.now();

  const tick = setInterval(() => {
    const total = Math.floor((Date.now() - started) / 1000);
    const mm = String(Math.floor(total / 60)).padStart(2, "0");
    const ss = String(total % 60).padStart(2, "0");
    setElapsed(`${mm}:${ss}`);
  }, 1000);
  onCleanup(() => clearInterval(tick));

  onMount(() => {
    // clear every layer of the app background so only the pill shows; the
    // window itself can come up larger than the bar on gtk-wayland
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
    const root = document.getElementById("root");
    if (root) {
      root.style.background = "transparent";
    }
  });

  const stop = () => {
    invoke("recbar_stop").catch(() => {});
  };

  return (
    <div class="recbar">
      <span class="recbar-dot" />
      <span class="recbar-time">{elapsed()}</span>
      <button type="button" class="recbar-stop" onClick={stop}>
        stop
      </button>
    </div>
  );
}

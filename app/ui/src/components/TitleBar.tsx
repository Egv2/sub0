/* macOS overlay title bar: native traffic lights float over this strip, the
 * rest is a drag region. Double-click toggles maximize like a native bar. */
export const IS_MAC = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);

export default function TitleBar() {
  if (!IS_MAC) return null;
  return (
    <div className="titlebar" data-tauri-drag-region>
      <span className="titlebar-title" data-tauri-drag-region>Sub0</span>
    </div>
  );
}

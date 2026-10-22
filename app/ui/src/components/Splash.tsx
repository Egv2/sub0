import { useEffect, useState } from "react";
import jelly from "../assets/trans-jelly.webm";

export default function Splash({ done }: { done: boolean }) {
  const [mounted, setMounted] = useState(true);

  useEffect(() => {
    if (!done) return;
    const t = setTimeout(() => setMounted(false), 550);
    return () => clearTimeout(t);
  }, [done]);

  if (!mounted) return null;
  return (
    <div className={"splash" + (done ? " closing" : "")}>
      <video src={jelly} autoPlay muted loop playsInline />
    </div>
  );
}

import { Fingerprint } from "lucide-react";
import logoUrl from "../../assets/logo.svg";
import { Button } from "../ui/Button";
import styles from "./LockScreen.module.css";

export function LockScreen({ onUnlock }: { onUnlock: () => void }) {
  return (
    <main className={styles.root} data-tauri-drag-region>
      <img src={logoUrl} alt="" className={styles.logo} draggable={false} />
      <h1 className={styles.title}>Coffer</h1>
      <Button variant="primary" icon={Fingerprint} onClick={onUnlock}>
        Unlock with Touch ID
      </Button>
    </main>
  );
}

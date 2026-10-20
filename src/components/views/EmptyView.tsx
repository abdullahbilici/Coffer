import logoUrl from "../../assets/logo.svg";
import styles from "./EmptyView.module.css";
import viewStyles from "./View.module.css";

export function EmptyView() {
  return (
    <div className={viewStyles.view}>
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={styles.root}>
        <img src={logoUrl} alt="" className={styles.logo} draggable={false} />
      </div>
    </div>
  );
}

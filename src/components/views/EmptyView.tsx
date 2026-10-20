// The mark without the app-icon tile, so it sits directly on the window background.
import markUrl from "../../assets/mark.svg";
import styles from "./EmptyView.module.css";
import viewStyles from "./View.module.css";

export function EmptyView() {
  return (
    <div className={viewStyles.view}>
      <div className={viewStyles.dragbar} data-tauri-drag-region />
      <div className={styles.root}>
        <img src={markUrl} alt="" className={styles.logo} draggable={false} />
      </div>
    </div>
  );
}

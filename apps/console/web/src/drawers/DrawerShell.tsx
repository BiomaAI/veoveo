import type { ReactNode } from "react";
import { X } from "lucide-react";

export function DrawerShell({
  title,
  subtitle,
  onClose,
  children
}: {
  title: string;
  subtitle: string;
  onClose: () => void;
  children: ReactNode;
}) {
  return (
    <div className="drawer-layer">
      <button className="drawer-scrim" aria-label="Close details" onClick={onClose} />
      <aside className="drawer" aria-label={`${title} details`}>
        <header>
          <div>
            <span>{subtitle}</span>
            <h2>{title}</h2>
          </div>
          <button className="icon-button" onClick={onClose} title="Close">
            <X size={18} />
          </button>
        </header>
        {children}
      </aside>
    </div>
  );
}

import type { UiSettings } from "../types";
import { Card, CardHead, IconBtn, Toggle } from "./ui";
import { langNames } from "../languages";
import {
  IconChevronRight,
  IconFileText,
  IconGear,
  IconGlobe,
  IconLanguages,
  IconSparkles,
} from "./icons";

interface StatusCardProps {
  settings: UiSettings;
  onOpenSettings: () => void;
  onToggleCleanup: (v: boolean) => void;
  onToggleTranslate: (v: boolean) => void;
}

export default function StatusCard({
  settings,
  onOpenSettings,
  onToggleCleanup,
  onToggleTranslate,
}: StatusCardProps) {
  return (
    <Card className="bento-status">
      <CardHead
        icon={<IconGear size={16} />}
        title="Ayarlar & Durum"
        right={
          <IconBtn className="sm" title="Ayarları aç" onClick={onOpenSettings}>
            <IconChevronRight size={15} />
          </IconBtn>
        }
      />
      <div className="stat-row">
        <span className="icon-badge">
          <IconGlobe size={17} />
        </span>
        <span className="stat-main">
          <div className="stat-title">Kaynak dili</div>
          <div className="stat-value">Otomatik algıla</div>
        </span>
      </div>
      <div className="stat-row">
        <span className="icon-badge">
          <IconLanguages size={17} />
        </span>
        <span className="stat-main">
          <div className="stat-title">Çeviri</div>
          <div className="stat-value">
            {settings.translate ? "Hedef dillere çevrilir" : "Yalnızca transkript"}
          </div>
        </span>
        <Toggle
          checked={settings.translate}
          onChange={onToggleTranslate}
          title="Çeviriyi aç/kapat"
        />
      </div>
      {settings.translate && (
        <div
          className="stat-row clickable"
          onClick={onOpenSettings}
          title="Hedef dilleri Ayarlar'dan düzenle"
        >
          <span className="icon-badge">
            <IconGlobe size={17} />
          </span>
          <span className="stat-main">
            <div className="stat-title">Hedef diller</div>
            <div className="stat-value">
              {settings.langs.length ? langNames(settings.langs) : "Dil seçilmedi"}
            </div>
          </span>
          <span className="stat-side">
            <IconChevronRight size={15} />
          </span>
        </div>
      )}
      <div className="stat-row">
        <span className="icon-badge">
          <IconFileText size={17} />
        </span>
        <span className="stat-main">
          <div className="stat-title">Altyazı formatı</div>
          <div className="stat-value">SRT</div>
        </span>
      </div>
      <div className="stat-row">
        <span className="icon-badge">
          <IconSparkles size={17} />
        </span>
        <span className="stat-main">
          <div className="stat-title">Dolgu temizliği</div>
          <div className="stat-value">
            {settings.cleanup ? "Açık" : "Kapalı"}
          </div>
        </span>
        <Toggle
          checked={settings.cleanup}
          onChange={onToggleCleanup}
          title="Dolgu temizliği"
        />
      </div>
    </Card>
  );
}

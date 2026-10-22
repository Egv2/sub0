import LanguageSelect from "./LanguageSelect";

interface LanguagePickerProps {
  pending: string[];
  existing: string[];
  onToggle: (code: string) => void;
  onTranslate: () => void;
}

export default function LanguagePicker({ pending, existing, onToggle, onTranslate }: LanguagePickerProps) {
  return (
    <>
      <LanguageSelect
        selected={pending}
        locked={existing}
        onToggle={onToggle}
        autoFocus
        footer={
          <button className="primary" disabled={pending.length === 0} onClick={onTranslate}>
            {pending.length > 0 ? `Çevir (${pending.length})` : "Çevir"}
          </button>
        }
      />
    </>
  );
}

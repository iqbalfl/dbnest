import { useState } from "react";

interface Props {
  instanceName: string;
  onCancel: () => void;
  onConfirm: (keepData: boolean) => Promise<void>;
}

export default function DeleteConfirmDialog({ instanceName, onCancel, onConfirm }: Props) {
  const [typed, setTyped] = useState("");
  const [keepData, setKeepData] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const matches = typed === instanceName;

  return (
    <div className="modal-backdrop" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Hapus "{instanceName}"?</h2>
        <p>
          Tindakan ini tidak bisa dibatalkan. Ketik <strong>{instanceName}</strong> untuk
          konfirmasi.
        </p>
        <input
          type="text"
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          placeholder={instanceName}
        />
        <label className="field field-checkbox">
          <input
            type="checkbox"
            checked={keepData}
            onChange={(e) => setKeepData(e.target.checked)}
          />
          <span>Simpan data (jangan hapus folder instance)</span>
        </label>
        <div className="modal-actions">
          <button className="btn" onClick={onCancel} disabled={submitting}>
            Batal
          </button>
          <button
            className="btn btn-danger"
            disabled={!matches || submitting}
            onClick={async () => {
              setSubmitting(true);
              await onConfirm(keepData);
              setSubmitting(false);
            }}
          >
            Hapus
          </button>
        </div>
      </div>
    </div>
  );
}

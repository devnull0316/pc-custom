import { useMemo, useState, type KeyboardEvent } from "react";

import type { IconName } from "../model";
import { Dialog } from "./Dialog";
import { Icon } from "./Icon";

export interface PaletteCommand {
  id: string;
  label: string;
  description: string;
  icon: IconName;
  execute: () => void;
}

interface CommandPaletteProps {
  commands: readonly PaletteCommand[];
  onClose: () => void;
}

export function CommandPalette({ commands, onClose }: CommandPaletteProps) {
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const filtered = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase("ja-JP");
    if (normalized.length === 0) return commands;
    return commands.filter((command) => `${command.label} ${command.description}`.toLocaleLowerCase("ja-JP").includes(normalized));
  }, [commands, query]);
  const safeIndex = Math.min(activeIndex, Math.max(filtered.length - 1, 0));

  function choose(index: number) {
    const command = filtered[index];
    if (command === undefined) return;
    command.execute();
    onClose();
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((current) => Math.min(current + 1, filtered.length - 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((current) => Math.max(current - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      choose(safeIndex);
    }
  }

  return (
    <Dialog
      description="画面や登録済みActionへ移動します。ここから変更を直接適用することはありません。"
      footer={<div className="palette-hints"><span><kbd>↑</kbd><kbd>↓</kbd>選択</span><span><kbd>Enter</kbd>開く</span><span><kbd>Esc</kbd>閉じる</span></div>}
      onClose={onClose}
      title="コマンドパレット"
      width="wide"
    >
      <label className="palette-search">
        <Icon name="search" />
        <span className="sr-only">結果を検索</span>
        <input
          autoComplete="off"
          data-dialog-autofocus=""
          onChange={(event) => { setQuery(event.target.value); setActiveIndex(0); }}
          onKeyDown={handleKeyDown}
          placeholder="やりたいことで検索"
          spellCheck="false"
          type="search"
          value={query}
        />
        <kbd>Ctrl K</kbd>
      </label>
      {filtered.length === 0 ? (
        <div className="palette-empty"><Icon name="search" /><strong>一致する結果はありません</strong><span>別の言葉で検索してください。</span></div>
      ) : (
        <div aria-label="検索結果" className="palette-results" role="listbox">
          {filtered.map((command, index) => (
            <button
              aria-selected={safeIndex === index}
              className="palette-result"
              key={command.id}
              onClick={() => choose(index)}
              onMouseEnter={() => setActiveIndex(index)}
              role="option"
              type="button"
            >
              <span><Icon name={command.icon} /></span>
              <span><strong>{command.label}</strong><small>{command.description}</small></span>
              <kbd>↵</kbd>
            </button>
          ))}
        </div>
      )}
    </Dialog>
  );
}

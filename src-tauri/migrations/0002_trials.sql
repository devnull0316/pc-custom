-- 試用: 適用してから一定時間内に「保存する」を押さなければ、自動で元へ戻す。
--
-- 見た目に関わる設定は、説明文を読んでも良し悪しが判断できない。
-- 実際に適用した状態を見てから決められるようにする。
-- アプリが落ちても取り残されないよう、期限は**メモリではなくここ**に持つ。
CREATE TABLE IF NOT EXISTS trials (
  transaction_id TEXT PRIMARY KEY REFERENCES transactions(transaction_id) ON DELETE CASCADE,
  expires_at_unix_ms INTEGER NOT NULL,
  -- 保存が押された時刻。NULL のままで期限を過ぎたものは、起動時に元へ戻す対象。
  confirmed_at_unix_ms INTEGER,
  created_at_unix_ms INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS trials_pending
  ON trials(expires_at_unix_ms) WHERE confirmed_at_unix_ms IS NULL;

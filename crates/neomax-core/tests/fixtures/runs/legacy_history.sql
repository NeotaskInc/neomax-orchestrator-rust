CREATE TABLE runs(
    id TEXT PRIMARY KEY, engine TEXT, account TEXT, acct_no INTEGER,
    status TEXT, prompt TEXT, repo TEXT, branch TEXT, tag TEXT, goal TEXT,
    effort TEXT, ultra INTEGER, opus INTEGER, model TEXT,
    children INTEGER, attempt INTEGER, pr_url TEXT,
    started INTEGER, ended INTEGER, archived_at INTEGER,
    log_path TEXT, record TEXT
);
CREATE INDEX runs_started ON runs(started DESC);
INSERT INTO runs(
    id, engine, account, acct_no, status, prompt, repo, branch, tag, goal,
    effort, ultra, opus, model, children, attempt, pr_url, started, ended,
    archived_at, log_path, record
) VALUES(
    'legacy-python-orch', 'claude', '.claude-orch', 'orch', 'done',
    'Inspect the service', 'service-a', 'neomax/legacy-python-orch',
    'compatibility', 'Keep the run recoverable', 'high', 0, 0,
    'claude-fable-5[1m]', 0, 1, NULL, 1787488000, 1787488400,
    1787488500, NULL, NULL
);
INSERT INTO runs(
    id, engine, account, acct_no, status, prompt, repo, branch, tag, goal,
    effort, ultra, opus, model, children, attempt, pr_url, started, ended,
    archived_at, log_path, record
) VALUES(
    'legacy-python-acct', 'codex', '.codex-acct12', 12, 'done',
    'Inspect the worker', 'worker-b', 'neomax/legacy-python-acct',
    'compatibility', 'Keep the account marker', 'high', 0, 0,
    'gpt-5.6-sol', 0, 2, NULL, 1787488100, 1787488500,
    1787488600, NULL, NULL
);

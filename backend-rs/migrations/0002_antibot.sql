CREATE TABLE fingerprints (
    visitor_id  text PRIMARY KEY,
    traits_hash text NOT NULL,
    ua_family   text,
    platform    text,
    headless    boolean NOT NULL DEFAULT false,
    first_seen  timestamptz NOT NULL DEFAULT now(),
    last_seen   timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE visitor_accounts (
    visitor_id text NOT NULL REFERENCES fingerprints(visitor_id) ON DELETE CASCADE,
    user_id    int  NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    paints     int NOT NULL DEFAULT 0,
    first_seen timestamptz NOT NULL DEFAULT now(),
    last_seen  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (visitor_id, user_id)
);
CREATE INDEX visitor_accounts_user_idx ON visitor_accounts (user_id);
CREATE INDEX visitor_accounts_visitor_idx ON visitor_accounts (visitor_id);
CREATE TABLE bot_flags (
    user_id    int PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    score      int NOT NULL DEFAULT 0,
    reasons    text[] NOT NULL DEFAULT '{}',
    updated_at timestamptz NOT NULL DEFAULT now()
);

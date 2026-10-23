-- A WeChat sign-in in progress. The browser only carries a random state; Clerk's
-- redirect, nonce and the profile stay here until the code is used or it expires.
CREATE TABLE wechat_oidc (
  state          TEXT PRIMARY KEY,
  client_id      TEXT NOT NULL,
  redirect_uri   TEXT NOT NULL,
  clerk_state    TEXT NOT NULL,
  nonce          TEXT NOT NULL,
  code_challenge TEXT,
  auth_code      TEXT,
  access_token   TEXT,
  subject        TEXT,
  name           TEXT,
  picture        TEXT,
  expires_at     INTEGER NOT NULL,
  consumed_at    INTEGER
);
CREATE UNIQUE INDEX wechat_oidc_auth_code ON wechat_oidc(auth_code);
CREATE UNIQUE INDEX wechat_oidc_access_token ON wechat_oidc(access_token);

CREATE TABLE users (
  id          uuid          NOT NULL,
  username    varchar(255)  NOT NULL,
  password    varchar(255)  NOT NULL,
  name        TEXT          NOT NULL,
  created_at  timestamptz   NOT NULL default now(),
  updated_at  timestamptz   NOT NULL default now(),
  deleted_at  timestamptz,

  PRIMARY KEY (id),
  CONSTRAINT unique_username UNIQUE (username)
);

CREATE TABLE api_token (
  id          uuid          NOT NULL,
  user_id     uuid          NOT NULL,
  token       TEXT          NOT NULL,
  created_at  timestamptz   NOT NULL default now(),
  updated_at  timestamptz   NOT NULL default now(),
  deleted_at  timestamptz,

  PRIMARY KEY (id),
  FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE ON UPDATE CASCADE
);

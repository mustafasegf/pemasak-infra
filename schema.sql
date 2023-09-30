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

CREATE TABLE owner (
  id          uuid          NOT NULL,
  name        TEXT          NOT NULL,
  created_at  timestamptz   NOT NULL default now(),
  updated_at  timestamptz   NOT NULL default now(),
  deleted_at  timestamptz,

  PRIMARY KEY (id)
);

CREATE TABLE users_owner (
  user_id     uuid          NOT NULL,
  owner_id    uuid          NOT NULL,
  created_at  timestamptz   NOT NULL default now(),
  updated_at  timestamptz   NOT NULL default now(),
  deleted_at  timestamptz,

  PRIMARY KEY (user_id, owner_id),
  FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE ON UPDATE CASCADE,
  FOREIGN KEY (owner_id) REFERENCES owner(id) ON DELETE CASCADE ON UPDATE CASCADE
);


CREATE TABLE repositories (
  id          uuid          NOT NULL,
  owner_id    uuid          NOT NULL,
  name        TEXT          NOT NULL,
  created_at  timestamptz   NOT NULL default now(),
  updated_at  timestamptz   NOT NULL default now(),
  deleted_at  timestamptz,

  PRIMARY KEY (id),
  FOREIGN KEY (owner_id) REFERENCES owner(id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE domains (
  id          uuid          NOT NULL,
  repo_id     uuid          NOT NULL,
  name        TEXT          NOT NULL,
  created_at  timestamptz   NOT NULL default now(),
  updated_at  timestamptz   NOT NULL default now(),
  deleted_at  timestamptz,

  PRIMARY KEY (id),
  FOREIGN KEY (repo_id) REFERENCES repositories(id) ON DELETE CASCADE ON UPDATE CASCADE
);


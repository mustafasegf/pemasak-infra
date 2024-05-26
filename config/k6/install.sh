#!/usr/bin/env bash
 docker run --rm -u "$(id -u):$(id -g)" -v "${PWD}:/xk6" grafana/xk6 build \
--with github.com/szkiba/xk6-yaml@latest \
--with github.com/mustafasegf/xk6-exec@latest \
--with github.com/grafana/xk6-timers@latest \
--with github.com/grafana/xk6-sql@latest \
--with github.com/acuenca-facephi/xk6-read@latest \
--with github.com/szkiba/xk6-dotenv@latest \
--with github.com/grafana/xk6-sql

import http from "k6/http";
import exec from "k6/x/exec";
import read from "k6/x/read";
import { sleep } from "k6";
import execution from "k6/execution";
import sql from 'k6/x/sql';

const csvData = open("./data-all.csv").trim();
const csvArr = csvData
  .split(/\r?\n/)
  .map((line) => line.split(","))
let csv = csvArr.map((line) => ({
  name: line[1].trim().replaceAll(" ", "").toLowerCase(),
  github: "https://" + line[2].trim(),
})).slice(0, 16);

const db = sql.open('postgres', 'postgresql://pemasak:Memet-Skibidi-Gyatt-69@localhost:5439/pemasak?sslmode=disable');

export let options = {
  setupTimeout: "60m",
  teardownTimeout: "60m",
  scenarios: {
    build: {
      executor: "shared-iterations",
      vus: 1,
      iterations: 2,
      maxDuration: "60m",
    },
  },
  thresholds: {
    'iteration_duration{scenario:default}': [`max>=0`],
    'iteration_duration{group:::setup}': [`max>=0`],
    'iteration_duration{group:::teardown}': [`max>=0`],
    'http_req_duration{scenario:default}': [`max>=0`],
  },
};

const { username, password, domain } = {
  "domain": "https://pbp.cs.ui.ac.id",
  "username": "adrian.ardizza",
  "password": "ardizza123",
};

export function setup() {
  console.log({ domain });
  console.log({ pwd: exec.command("pwd") });

  const dbIdMap = {}

  // make sure git auth is disable

  // login and get cookie
  const loginRes = http.post(
    domain + "/api/login",
    JSON.stringify({
      username,
      password,
    }),
    { headers: { "Content-Type": "application/json" } },
  );

  console.log({
    loginStatus: loginRes.status,
  });

  if (loginRes.status !== 302) {
    console.log("login failed");
    return;
  }

  const cookies = loginRes.cookies;
  const cookieString = Object.keys(cookies)
    .map((name) => {
      return `${name}=${cookies[name][0].value}`;
    })
    .join("; ");

  console.log({ cookieString });
  // console.log({ loginRes });

  for (const { name, github } of csv) {
    console.log({ name, domain });

    // check if project already exists
    try {
      read.readDirectory("./clone/" + name);
      console.log("project already exists");
    } catch (error) {
      console.log({ error });
      // clone github repo to clone folder
      console.log("cloning repo", { name, github });

      try {
        exec.command("git", ["clone", github, name], {
          dir: "clone",
          fatalError: false,
        });
      } catch (error) {
        console.log("error cloning repo", github, { error });
        csv = csv.filter((item) => item.name !== name);
        continue;
      }

      // change main branch to master
      console.log("changing main branch");
      exec.command("git", ["branch", "-M", "master"], {
        dir: "clone/" + name,
        fatalError: false,
      });

      // add remote
      console.log("adding remote");
      exec.command(
        "git",
        ["remote", "add", "pws", domain + "/" + username + "/" + name],
        {
          dir: "clone/" + name,
          fatalError: false,
        },
      );
    }

    // create project
    const createRes = http.post(
      domain + "/api/project/new",
      JSON.stringify({ owner: username, project: name }),
      {
        headers: {
          Cookie: cookieString,
          "Content-Type": "application/json",
        },
      },
    );
    const parsedRes = JSON.parse(createRes.body)
    dbIdMap[name] = parsedRes.id
  }


  console.log(dbIdMap)
  return { cookieString, dbIdMap }
}

export default async function ({ cookieString, dbIdMap }) {
  const testData = csv[execution.scenario.iterationInTest]
  const { name, github } = testData

  console.log(`pushing project ${name} to pws`);

  const execRes = exec.command("git", ["push", "-u", "pws", "master"], {
    dir: "clone/" + name,
    fatalError: false,
  });

  while (true) {
    sleep(1 + Math.random() * 1)
    const results = sql.query(db, "SELECT status FROM builds WHERE project_id = $1;", dbIdMap[name])    
    const status = String.fromCharCode.apply(null, results[0].status)


    if (status === "successful") {
      console.log(`project ${name} deployed`);
      break
    }

    if (status === "failed") {
      console.log(`project ${name} failed to deploy`);
      break
    }
  }
}

export function teardown() {
  // login and get cookie
  const loginRes = http.post(
    domain + "/api/login",
    JSON.stringify({
      username,
      password,
    }),
    { headers: { "Content-Type": "application/json" } },
  );

  console.log({
    loginStatus: loginRes.status,
  });

  if (loginRes.status !== 302) {
    console.log("login failed");
    return;
  }

  const cookies = loginRes.cookies;
  const cookieString = Object.keys(cookies)
    .map((name) => {
      return `${name}=${cookies[name][0].value}`;
    })
    .join("; ");

  let promisesDelete = [];

  for (const { name, github } of csv) {
    promisesDelete.push(
      new Promise((resolve, reject) => {
        // delete project
        console.log({
          domain: `${domain}/api/project/${username}/${name}/delete`,
        });

        const deleteRes = http.post(
          `${domain}/api/project/${username}/${name}/delete`,
          null,
          {
            headers: {
              Cookie: cookieString,
              "Content-Type": "application/json",
            },
          },
        );
        if (deleteRes.status !== 200) {
          console.log(`error deleting project ${name}`);
          reject();
          return;
        }

        console.log(`project ${name} deleted`);
        resolve();
      }),
    );
  }

  Promise.allSettled(promisesDelete)
    .then((results) => {
      console.log("all project deleted");
    })
    .catch((error) => {
      console.log("tear down error", { error });
    });
}

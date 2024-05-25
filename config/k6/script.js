import http from "k6/http";
import exec from "k6/x/exec";
import read from "k6/x/read";
import dotenv from "k6/x/dotenv";

export let options = {
  setupTimeout: "60m",
};

const csvData = open("./data-all.csv").trim();
const csvArr = csvData
  .split(/\r?\n/)
  .map((line) => line.split(","))
  // .slice(0, 1);
let csv = csvArr.map((line) => ({
  name: line[1].trim().replaceAll(" ", "").toLowerCase(),
  github: "https://" + line[2].trim(),
}));

const { username, password, domain } = dotenv.parse(open("./.env"));

export function setup() {
  console.log({ domain });
  console.log({ pwd: exec.command("pwd") });

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
      });

      // add remote
      console.log("adding remote");
      exec.command(
        "git",
        ["remote", "add", "pws", domain + "/" + username + "/" + name],
        {
          dir: "clone/" + name,
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
  }
}

export default function() {
  const buildPromises = csv.map(
    ({ name, github }) =>
      new Promise((resolve, reject) => {
        // push to pws
        console.log(`pushing project ${name} to pws`);
        const execRes = exec.command("git", ["push", "-u", "pws", "master"], {
          dir: "clone/" + name,
        });

        // check if project is deployed
        resolve();
      }),
  );

  Promise.allSettled(buildPromises)
    .then((results) => {
      console.log("all project created");
    })
    .catch((error) => {
      console.log("error happened when creating project", { error });
    });
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
  const promisesDelete = csv.map(
    ({ name, github }) =>
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
  Promise.allSettled(promisesDelete)
    .then((results) => {
      console.log("all project deleted");
    })
    .catch((error) => {
      console.log("tear down error", { error });
    });
}

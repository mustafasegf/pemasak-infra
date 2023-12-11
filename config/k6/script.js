// import necessary module
import * as YAML from "k6/x/yaml";
import http from "k6/http";
import exec from 'k6/x/exec';
import read from 'k6/x/read';
import dotenv from "k6/x/dotenv";

// const config = YAML.parse(open('../../configuration.yml'));
// let domain = config.application.domain;

const csvData = open('./data.csv').trim();
const csvArr = csvData.split(/\r?\n/).map((line) => line.split(',')).slice(0, 1);
const csv = csvArr.map(line => ({ name: line[1].trim().replace(" ", "").toLowerCase(), github: "https://" + line[2].trim() }))

const { username, password, domain } = dotenv.parse(open("./.env"));

export default function() {
  console.log({ domain });
  console.log({ pwd: exec.command('pwd') });

  // make sure git auth is disable

  // login and get cookie
  let loginRes = http.post(domain + "/login", {
    username,
    password
  });

  let cookies = loginRes.cookies;
  let cookieString = Object.keys(cookies).map(name => {
    return `${name}=${cookies[name][0].value}`;
  }).join('; ');

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
      exec.command('git', ['clone', github, name], {
        "dir": "clone"
      });


      // change main branch to master
      console.log("changing main branch");
      exec.command('git', ['branch', '-M', 'master'], {
        "dir": "clone/" + name
      });

      // add remote
      console.log("adding remote");
      exec.command('git', ['remote', 'add', 'pws', domain + "/" + username + "/" + name], {
        "dir": "clone/" + name
      });

    }

    // create project
    let res = http.post(domain + "/new", { owner: username, project: name }, {
      headers: {
        "Cookie": cookieString
      }
    });
    // console.log(res.body);


    // push to pws
    console.log("pushing to pws");
    let execRes = exec.command('git', ['push', '-u', 'pws', 'master'], {
      "dir": "clone/" + name
    });
    console.log({ execRes });

    // delete project
    http.post(`${domain}/${username}/${name}/delete`, {}, {
      headers: {
        "Cookie": cookieString
      }
    })

  }
}


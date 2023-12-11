// import necessary module
import * as YAML from "k6/x/yaml";
import http from "k6/http";
import exec from 'k6/x/exec';
import read from 'k6/x/read';

const config = YAML.parse(open('../../configuration.yml'));
const csvData = open('./data.csv').trim();
const csvArr = csvData.split(/\r?\n/).map((line) => line.split(',')).slice(0, 1);
const csv = csvArr.map(line => ({ name: line[1].trim().replace(" ", "-").toLowerCase(), github: "https://" + line[2].trim() }))

export default async function() {
  let domain = config.application.domain;
  console.log({ domain });
  console.log({ pwd: exec.command('pwd') });


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
    }

    // create project
    http.post(domain + "/new", {

    // add remote

    // push to pws
  }

  // // define URL and payload
  // const url = "https://test-api.k6.io/auth/basic/login/";
  // const payload = JSON.stringify({
  //   username: "test_case",
  //   password: "1234",
  // });
  //
  // const params = {
  //   headers: {
  //     "Content-Type": "application/json",
  //   },
  // };
  //
  // // send a post request and save response as a variable
  // const res = http.post(url, payload, params);
  // console.log(res.body);
}


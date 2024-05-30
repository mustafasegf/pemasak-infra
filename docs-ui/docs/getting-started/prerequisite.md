---
sidebar_position: 1
---

# Intro and Prerequisite

Introduction and list of items you need to prepare before deploying.

## Welcome to PWS Tutorial
To get you started, we're going to provide you a step by step tutorial for deploying a simple [Django](https://www.djangoproject.com/) application as an example, from start to finish.

## Prerequisites
Before starting, make sure you have prepared the following items
1. Django Application.
2. Ensure you have `gunicorn` in `requirement.txt`. Example is below:   

    ```
    ...
    PySocks==1.7.1
    requests==2.31.0
    selenium==3.14.0
    sniffio==1.3.0
    sortedcontainers==2.4.0
    sqlparse==0.4.4
    trio==0.22.2
    trio-websocket==0.10.3
    tzdata==2023.3
    urllib3==2.0.3
    whitenoise==6.5.0
    wsproto==1.2.0
    gunicorn==21.2.0  <- Make sure you have this
    ```    
3. Ensure there is no `package.json` or `package-lock.json` in your repository and project folder.

:::tip Some Tips
 If you have already managed to deploy to DOKKU, everything should be fine, but make sure to check all points in prerequisite.
:::
---
sidebar_position: 1
---


# Fix CORS Issue

Fix CORS Issue in your Django Project when it is deployed to PWS.

## What is CORS?

You can read documentation regarding CORS in the [MDN Web](https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS)

## How to Fix in Django Project?

1. Go to your Django project folder.
2. In your project's `settings.py`, ensure that you have the following line.    
    ```
    CSRF_TRUSTED_ORIGINS = ["https://*.stndar.dev"] 
    ```
3. Save and commit the changes.
    ```git add .
    git commit -m "bugfix:add CSRF trusted origin"
    ```
4. Push the changes to PWS.    
    ```
    git push pws master
    ```

    :::tip Same Push Command

    Note that this uses the same push command as deploying. If you want to do changes, you can use this same command everytime you've made some changes in your computer.

    :::

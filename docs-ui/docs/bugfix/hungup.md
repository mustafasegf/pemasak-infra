---
sidebar_position: 2
---

# Fix Git Unexpected Error

You might encounter the following error when pushing your service to PWS.

```
Enumerating objects: 1453, done.
Counting objects: 100% (1453/1453), done.
Delta compression using up to 8 threads
Compressing objects: 100% (575/575), done.
error: RPC failed; HTTP 500 curl 22 The requested URL returned error: 500
send-pack: unexpected disconnect while reading sideband packet
Writing objects: 100% (1453/1453), 14.86 MiB | 127.91 MiB/s, done.
Total 1453 (delta 841), reused 1444 (delta 836), pack-reused 0
fatal: the remote end hung up unexpectedly
Everything up-to-date
```

## Solving the Error

If you encounter this, simply run the following command in the terminal directed to your project directory

```
git config --global http.postBuffer 524288000
```

Then you can push your changes again by running the git push command

```
git push pws master
```

The error should be resolved now.


### Further Reading
[Stack Overflow - Github - unexpected disconnect while reading sideband packet](https://stackoverflow.com/questions/66366582/github-unexpected-disconnect-while-reading-sideband-packet)
#!/usr/bin/env bash
# Delopy and restart toolforge services
ssh login.toolforge.org 'become catfood bash -lc "cd ~ && git pull && ./build.sh && ./restart.sh && echo \"OK\""'

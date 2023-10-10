#!/usr/bin/env bash

# install nix, need sudo
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install

# source nix
source /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh

# install direnv
nix profile install nixpkgs#direnv

# install nix-direnv
nix profile install nixpkgs#nix-direnv

# put shell hook in .bashrc and .zshrc
if [ -f ~/.bashrc ]; then
  echo 'eval "$(direnv hook bash)"' >> ~/.bashrc
fi

if [ -f ~/.zshrc ]; then
  echo 'eval "$(direnv hook zsh)"' >> ~/.zshrc
fi

# add nix-direnv to .config/direnv/direnvrc
if [ ! -d ~/.config/direnv ]; then
  mkdir -p ~/.config/direnv
fi

echo 'source $HOME/.nix-profile/share/nix-direnv/direnvrc' >> ~/.config/direnv/direnvrc

# Refresh the databases the desktop reads, so the association works without a
# logout. Fedora runs all three from rpm file triggers and Debian runs them
# from dpkg triggers, which makes these belt and braces on both and the whole
# story on distributions that ship neither.
# Every line ends in `|| :` because a package must not fail to install over a
# cache refresh.
update-mime-database /usr/share/mime >/dev/null 2>&1 || :
update-desktop-database /usr/share/applications >/dev/null 2>&1 || :
gtk-update-icon-cache -qtf /usr/share/icons/hicolor >/dev/null 2>&1 || :

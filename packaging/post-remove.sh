# Our MIME definition has just gone; rebuild the database so nothing keeps
# claiming .excalidraw files on our behalf. Runs on upgrades too, where the
# replacement file is already in place and this is simply a no-op refresh.
update-mime-database /usr/share/mime >/dev/null 2>&1 || :
update-desktop-database /usr/share/applications >/dev/null 2>&1 || :

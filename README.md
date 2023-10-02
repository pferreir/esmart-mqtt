# eSmart - MQTT Bridge

## Features
 * Reads data from eSmart through their XMPP API (you'll need a rooted phone to get the password!)
 * Writes to an MQTT broker
 * Home Assistant auto-discovery
 * Built-in throttling to avoid spamming the broker
 * Written in Rust 🦀

## History

The eSmart is an all-in-one domotics solution made in Switzerland, which lets you regulate heating, track energy/water consumption and works as a doorbell and intercom, among others.
Unfortunately, as of now, it doesn't provide an open API which can integrate with other systems. I did contact them on that, to no avail. So, I took it as my mission to reverse-engineer
their API and provide an MQTT-compatible bridge. Fortunately, there was already some information online (see "Acknowledgements"), which made things easier.

## Running

The following values have to be obtained from the eSmart app, using a rooted phone (see following section for instructions on how to do it):

| CLI argument   | Env. var  | Meaning | Default |
| -------------- |-----------|---------|---------|
| --xmpp-jid     | XMPP_JID  | JID + `@myesmart.net` ||
| --xmpp-room    | XMPP_ROOM | DEVICE_ID + `@conference.myesmart.net` ||
| --xmpp-password | XMPP_PASSWORD | The XMPP password used by the mobile app ||
| --xmpp-nickname | XMPP_NICKNAME | Nickname to use in the XMPP room (arbitrary AFAIK) | `esmarter` |
| --mqtt-hostname | MQTT_HOSTNAME | Host name of your MQTT broker ||
| --mqtt-port     | MQTT_PORT | Port where your broker is running | 1883 |
| --mqtt-id       | MQTT_ID   | ID used to identify the device with the MQTT broker | `esmarter` | 
| --mqtt-throttling-secs | MQTT_THROTTLING_SECS | how many seconds to keep between MQTT updates. values which are received between updates are averaged across this period | 300

The ideal way to run the program is probably by placing the configuration settings in environment variables, but you can also pass arguments through the CLI, e.g.:

```sh
$ esmart_mqtt --xmpp-nickname="the_watcher"
```

The help option (`-h`) describes each option quite well.

Builds are provided for `x86_64`, `armv7` and `aarch64`, but any architecture/OS supported by the underlying crates should be OK.

### Getting the JID/password (rooted phone needed!)

First of all, get a **rooted Android phone**, then install the eSmart App from the Play Store. Then, follow the usual app onboarding process to link your phone to the eSmart. Once that's done, use ADB to read the authentication parameters from the phone:

```sh
$ adb root
$ adb shell
OnePlus5T:/ # cat /data/data/ch.myesmart.esmartlive/shared_prefs/prefs_secure_info.xml
```

You should get output which resembles this:

```xml
<?xml version='1.0' encoding='utf-8' standalone='yes' ?>
<map>
    <string name="prefs_connected_devices">[{&quot;id&quot;:&quot;xxxxxxxxxxxxxx&quot;,&quot;isEnable&quot;:true,&quot;name&quot;:&quot;xxxxxxxxxxxxxxxxxxxx&quot;}]</string>
    <string name="prefs_secure_hash">[this is your PASSWORD]</string>
    <string name="prefs_jid">[this is your JID, without the `@myesmart.net` suffix]</string>
    <string name="prefs_selected_device">[this is the DEVICE_ID. It can be used to generate the XMPP_ROOM, by adding the suffix `@conference.myesmart.net`]</string>
</map>
```

## Building from source

```sh
$ cargo build --release
```

## TODO

* [ ] Reverse-engineering the app registration process (no need for a rooted phone anymore?)

## Acknowledgements

* Nils Amiet - "The smart home I didn't ask for" - https://www.youtube.com/watch?v=CE7jRpUa29k

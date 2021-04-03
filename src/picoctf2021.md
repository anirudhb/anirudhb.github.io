---
title: PicoCTF 2021 writeups
date: 04/03/2021
time_to_read: ~ # TODO
---

# Introduction

These are my writeups for PicoCTF 2021!

All of these challenges assume that the target computer is running 64-bit Linux.
I've hopefully written these to be a gentle introduction to CTFs, and tried to elaborate as much as possible.
If you don't understand something it's always great to Google it!

# General Skills

## Obedient Cat

_5 points_

Download [the flag](https://mercury.picoctf.net/static/0e428b2db9788d31189329bed089ce98/flag) to get the flag :p

Flag: `picoCTF{s4n1ty_v3r1f13d_2fd6ed29}`

## Python Wrangling

_10 points_

Download the Python script, password and flag.

```
$ curl -LO https://mercury.picoctf.net/static/325a52d249be0bd3811421eacd2c877a/ende.py
$ curl -LO https://mercury.picoctf.net/static/325a52d249be0bd3811421eacd2c877a/pw.txt
$ curl -LO https://mercury.picoctf.net/static/325a52d249be0bd3811421eacd2c877a/flag.txt.en
```

Then we run the script with `-h` to get help:

```
$ python ende.py -h
Usage: ende.py (-e/-d) [file]
Examples:
  To decrypt a file named 'pole.txt', do: '$ python ende.py -d pole.txt'

```

Here we can see that we need to just run `python ende.py -d flag.txt.en` using the password from `pw.txt`:

```
$ python ende.py -d flag.txt.en
Please enter the password:ac9bd0ffac9bd0ffac9bd0ffac9bd0ff
picoCTF{redacted}
```

Flag: `picoCTF{4p0110_1n_7h3_h0us3_ac9bd0ff}`

## Wave a flag

_10 points_

We download the given program:

```
$ curl -LO https://mercury.picoctf.net/static/f95b1ee9f29d631d99073e34703a2826/warm
```

Next, since the program is executable we have to set the proper permissions:

```
$ chmod +x warm
```

Now we have to run the program. Since we don't know what it does, we'll use `-h`:

```
$ ./warm -h
Oh, help? I actually don't do much, but I do have this flag here: picoCTF{redacted}
```

Flag: `picoCTF{b1scu1ts_4nd_gr4vy_f0668f62}`

## Nice netcat...

_15 points_

We just run the netcat command provided:

```
$ nc mercury.picoctf.net 21135
112
105
99
111
67
84
70
123
103
48
48
100
95
107
49
116
116
121
33
95
110
49
99
51
95
107
49
116
116
121
33
95
97
102
100
53
102
100
97
52
125
10
```

Looks like it is printing the decimal value of each character of the flag.

For reference, remember that characters are encoded using ASCII. Here's a helpful table:
![ASCII table](https://www.asciitable.com/index/asciifull.gif)

Let's write a simple Python script that connects to the server and decodes the characters.
I'm using pwntools (which has documentation [here](https://docs.pwntools.com/en/latest).)
I'll also be using pwntools for the rest of the exploits so it's a good idea to read up on its usage.
Anyways, the script:

```py
from pwn import *

conn = remote("mercury.picoctf.net", 21135)
# Decode and split into lines
encoded = conn.recvall().strip().decode("ascii").split("\n")
decoded = ""
for number in encoded:
    # Turn into number
    number = int(number)
    # Add to decoded
    decoded += chr(number)
info(f"Flag: {decoded}")
```

Flag: `picoCTF{g00d_k1tty!_n1c3_k1tty!_afd5fda4}`

## Static ain't always noise

_20 points_

We first downloaded the given binary and script:

```
$ curl -LO https://mercury.picoctf.net/static/0f6ea599582dcce7b4f1ba94e3617baf/static
$ curl -LO https://mercury.picoctf.net/static/0f6ea599582dcce7b4f1ba94e3617baf/ltdis.sh
```

Let's make the script executable, and run it with no arguments to see how to use it:

```
$ chmod +x ltdis.sh
$ ./ltdis.sh
Attempting disassembly of  ...
objdump: 'a.out': No such file
objdump: section '.text' mentioned in a -j option, but not found in any input file
Disassembly failed!
Usage: ltdis.sh <program-file>
Bye!
```

Ah. We have to give it a program. Let's give it the static file:

```
$ ./ltdis.sh static
Attempting disassembly of static ...
Disassembly successful! Available at: static.ltdis.x86_64.txt
Ripping strings from binary with file offsets...
Any strings found in static have been written to static.ltdis.strings.txt with file offset
```

Now we can search the strings for the flag wrapper (`picoCTF{`):

```
$ grep picoCTF{ static.ltdis.strings.txt
   1020 picoCTF{redacted}
```

Flag: `picoCTF{d15a5m_t34s3r_6f8c8200}`

## Tab, Tab, Attack

_20 points_

Looks like we have a zip file. Let's download and unzip it:

```
$ curl -LO https://mercury.picoctf.net/static/9689f2b453ad5daeb73ca7534e4d1521/Addadshashanammu.zip
$ unzip Addadshashanammu.zip
Archive:  Addadshashanammu.zip
   creating: Addadshashanammu/
   creating: Addadshashanammu/Almurbalarammi/
   creating: Addadshashanammu/Almurbalarammi/Ashalmimilkala/
   creating: Addadshashanammu/Almurbalarammi/Ashalmimilkala/Assurnabitashpi/
   creating: Addadshashanammu/Almurbalarammi/Ashalmimilkala/Assurnabitashpi/Maelkashishi/
   creating: Addadshashanammu/Almurbalarammi/Ashalmimilkala/Assurnabitashpi/Maelkashishi/Onnissiralis/
   creating: Addadshashanammu/Almurbalarammi/Ashalmimilkala/Assurnabitashpi/Maelkashishi/Onnissiralis/Ularradallaku/
  inflating: Addadshashanammu/Almurbalarammi/Ashalmimilkala/Assurnabitashpi/Maelkashishi/Onnissiralis/Ularradallaku/fang-of-haynekhtnamet
```

Looks like there's a file in there.
Unfortunately, typing out the whole file name will take forever, so let's use the Tab key instead.
In most shells, the Tab key will autocomplete the name of the file or directory that you want.

So for example, if I pressed Tab here:

```
$ cat Addadshashanammu/
```

It would autocomplete to:

```
$ cat Addadshashanammu/Almurbalarammi/
```

So using this we can easily Tab to the file. Use file to get the file type:

```
$ file Addadshashanammu/Almurbalarammi/Ashalmimilkala/Assurnabitashpi/Maelkashishi/Onnissiralis/Ularradallaku/fang-of-haynekhtnamet
Addadshashanammu/Almurbalarammi/Ashalmimilkala/Assurnabitashpi/Maelkashishi/Onnissiralis/Ularradallaku/fang-of-haynekhtnamet: ELF 64-bit LSB shared object, x86-64, version 1 (SYSV), dynamically linked, interpreter /lib64/ld-linux-x86-64.so.2, for GNU/Linux 3.2.0, BuildID[sha1]=72a56ba85df661b5a985999a435927c01095cccf, not stripped
```

Looks like it's a program. Let's make it executable and run it:

```
$ chmod +x Addadshashanammu/Almurbalarammi/Ashalmimilkala/Assurnabitashpi/Maelkashishi/Onnissiralis/Ularradallaku/fang-of-haynekhtnamet
$ Addadshashanammu/Almurbalarammi/Ashalmimilkala/Assurnabitashpi/Maelkashishi/Onnissiralis/Ularradallaku/fang-of-haynekhtnamet
*ZAP!* picoCTF{redacted}
```

Flag: `picoCTF{l3v3l_up!_t4k3_4_r35t!_2bcfb2ab}`

## Magikarp Ground Mission

_30 points_

This challenge involves an instance.
Looks like we have to SSH into the instance with the username `ctf-player` and password `a13b7f9d`.

If you don't know too much about SSH, you can read more about it [here](https://www.digitalocean.com/community/tutorials/ssh-essentials-working-with-ssh-servers-clients-and-keys).

Let's SSH into the instance, with the provided command:

```
$ ssh ctf-player@venus.picoctf.net -p 49409
The authenticity of host '[venus.picoctf.net]:49409 ([3.131.124.143]:49409)' can't be established.
ECDSA key fingerprint is SHA256:NrQkIxNEQQho/GA7jE0WlIa7Jh4VF9sAvC5awkbuj1Q.
Are you sure you want to continue connecting (yes/no)? yes
Warning: Permanently added '[venus.picoctf.net]:49409,[3.131.124.143]:49409' (ECDSA) to the list of known hosts.
ctf-player@venus.picoctf.net's password: a13b7f9d
Welcome to Ubuntu 18.04.5 LTS (GNU/Linux 5.4.0-1041-aws x86_64)

 * Documentation:  https://help.ubuntu.com
 * Management:     https://landscape.canonical.com
 * Support:        https://ubuntu.com/advantage
This system has been minimized by removing packages and content that are
not required on a system that users do not log into.

To restore this content, you can run the 'unminimize' command.

The programs included with the Ubuntu system are free software;
the exact distribution terms for each program are described in the
individual files in /usr/share/doc/*/copyright.

Ubuntu comes with ABSOLUTELY NO WARRANTY, to the extent permitted by
applicable law.

ctf-player@pico-chall$
```

Let's run `ls` to poke around a little (I'm using `chall$` to denote that we are in SSH):

```
chall$ ls
1of3.flag.txt  instructions-to-2of3.txt
```

Ooh, let's look at both of those files:

```
chall$ cat 1of3.flag.txt
picoCTF{xxsh_
chall$ cat instructions-to-2of3.txt
Next, go to the root of all things, more succinctly `/`
```

Okay, let's try that and take a look there:

```
chall$ cd /
chall$ ls
2of3.flag.txt  bin  boot  dev  etc  home  instructions-to-3of3.txt  lib  lib64 media  mnt  opt  proc  root  run  sbin                                                                            srv  sys  tmp  usr  var
```

Ahah! Let's look at those files again...

```
chall$ cat 2of3.flag.txt
0ut_0f_\/\/4t3r_
chall$ cat instructions-to-3of3.txt
Lastly, ctf-player, go home... more succinctly `~`
```

Let's do that and take a look again:

```
chall$ cd ~
chall$ ls
3of3.flag.txt  drop-in
```

Yes! We have the last part of the flag:

```
chall$ cat 3of3.flag.txt
71be5264}
```

Let's combine all three parts togther.

Flag: `picoCTF{xxsh_0ut_0f_\/\/4t3r_71be5264}`

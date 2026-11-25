# Third-party material

## English word list (Spell Check)

`desktop/assets/spell/words.txt` is the built-in dictionary of Edit > Spell
Check. It is derived from SCOWL (Spell Checker Oriented Word Lists) by Kevin
Atkinson, http://wordlist.aspell.net/ (formerly wordlist.sourceforge.net),
through the Hunspell en_US dictionary built from SCOWL 2020.12.07 (size 60,
American spelling), as packaged by Debian (`hunspell-en-us`
1:2020.12.07-2).

How it was made: `tools/spell/build_wordlist.py` expanded the Hunspell stems
with the affix rules of `en_US.aff`, dropped words with digits or symbols,
one-letter words and possessives, sorted the result and front coded it (130,788
words, 591,000 bytes). Inputs and output, SHA-256:

- `en_US.dic` 829a043cf078d1e80e886289a13823454977f442a239a859d2133ea61944aa60
- `en_US.aff` 70fe5778717d097ce2f3326baaa5c1e4d2206d81a5a81d3ea8e11c4770806dd5
- `words.txt` 8e62be146fb0b3d1f71ae32ed988920030a74a99e33d712553aa0eb96ab8b930

License: the SCOWL license is permissive (use, copy, modify, distribute and
sell, with the copyright and permission notices kept in the documentation).
Its notices, with those of the sources SCOWL credits, follow verbatim from
the package's copyright file.

`desktop/assets/spell/construction.txt` (construction and project controls
words) was written for OpenXER and is not third-party material.

```text
SCOWL (Spell Checker Oriented Word Lists) is a collection of 
English word lists maintained by Kevin Atkinson 
<kevina@users.sourceforge.net>

It was downloaded from:
http://prdownloads.sourceforge.net/wordlist/scowl-5.tar.gz

SF project page:
http://wordlist.sourceforge.net/

Copyright: (extracted from the SCOWL README file):

The collective work is Copyright 2000-2011 by Kevin Atkinson as well
as any of the copyrights mentioned below:

  Copyright 2000-2011 by Kevin Atkinson

  Permission to use, copy, modify, distribute and sell these word
  lists, the associated scripts, the output created from the scripts,
  and its documentation for any purpose is hereby granted without fee,
  provided that the above copyright notice appears in all copies and
  that both that copyright notice and this permission notice appear in
  supporting documentation. Kevin Atkinson makes no representations
  about the suitability of this array for any purpose. It is provided
  "as is" without express or implied warranty.

Alan Beale <biljir@pobox.com> also deserves special credit as he has,
in addition to providing the 12Dicts package and being a major
contributor to the ENABLE word list, given me an incredible amount of
feedback and created a number of special lists (those found in the
Supplement) in order to help improve the overall quality of SCOWL.

The 10 level includes the 1000 most common English words (according to
the Moby (TM) Words II [MWords] package), a subset of the 1000 most
common words on the Internet (again, according to Moby Words II), and
frequently class 16 from Brian Kelk's "UK English Wordlist
with Frequency Classification".

The MWords package was explicitly placed in the public domain:

    The Moby lexicon project is complete and has
    been place into the public domain. Use, sell,
    rework, excerpt and use in any way on any platform.

    Placing this material on internal or public servers is
    also encouraged. The compiler is not aware of any
    export restrictions so freely distribute world-wide.

    You can verify the public domain status by contacting

    Grady Ward
    3449 Martha Ct.
    Arcata, CA  95521-4884

    grady@netcom.com
    grady@northcoast.com

The "UK English Wordlist With Frequency Classification" is also in the
Public Domain:

  Date: Sat, 08 Jul 2000 20:27:21 +0100
  From: Brian Kelk <Brian.Kelk@cl.cam.ac.uk>

  > I was wondering what the copyright status of your "UK English
  > Wordlist With Frequency Classification" word list as it seems to
  > be lacking any copyright notice.

  There were many many sources in total, but any text marked
  "copyright" was avoided. Locally-written documentation was one
  source. An earlier version of the list resided in a filespace called
  PUBLIC on the University mainframe, because it was considered public
  domain.

  Date: Tue, 11 Jul 2000 19:31:34 +0100

  > So are you saying your word list is also in the public domain?

  That is the intention.

The 20 level includes frequency classes 7-15 from Brian's word list.

The 35 level includes frequency classes 2-6 and words appearing in at
least 11 of 12 dictionaries as indicated in the 12Dicts package.  All
words from the 12Dicts package have had likely inflections added via
my inflection database.

The 12Dicts package and Supplement is in the Public Domain.

The WordNet database, which was used in the creation of the
Inflections database, is under the following copyright:

  This software and database is being provided to you, the LICENSEE,
  by Princeton University under the following license.  By obtaining,
  using and/or copying this software and database, you agree that you
  have read, understood, and will comply with these terms and
  conditions.:

  Permission to use, copy, modify and distribute this software and
  database and its documentation for any purpose and without fee or
  royalty is hereby granted, provided that you agree to comply with
  the following copyright notice and statements, including the
  disclaimer, and that the same appear on ALL copies of the software,
  database and documentation, including modifications that you make
  for internal use or for distribution.

  WordNet 1.6 Copyright 1997 by Princeton University.  All rights
  reserved.

  THIS SOFTWARE AND DATABASE IS PROVIDED "AS IS" AND PRINCETON
  UNIVERSITY MAKES NO REPRESENTATIONS OR WARRANTIES, EXPRESS OR
  IMPLIED.  BY WAY OF EXAMPLE, BUT NOT LIMITATION, PRINCETON
  UNIVERSITY MAKES NO REPRESENTATIONS OR WARRANTIES OF MERCHANT-
  ABILITY OR FITNESS FOR ANY PARTICULAR PURPOSE OR THAT THE USE OF THE
  LICENSED SOFTWARE, DATABASE OR DOCUMENTATION WILL NOT INFRINGE ANY
  THIRD PARTY PATENTS, COPYRIGHTS, TRADEMARKS OR OTHER RIGHTS.

  The name of Princeton University or Princeton may not be used in
  advertising or publicity pertaining to distribution of the software
  and/or database.  Title to copyright in this software, database and
  any associated documentation shall at all times remain with
  Princeton University and LICENSEE agrees to preserve same.

The 40 level includes words from Alan's 3esl list found in version 4.0
of his 12dicts package.  Like his other stuff the 3esl list is also in the
public domain.

The 50 level includes Brian's frequency class 1, words words appearing
in at least 5 of 12 of the dictionaries as indicated in the 12Dicts
package, and uppercase words in at least 4 of the previous 12
dictionaries.  A decent number of proper names is also included: The
top 1000 male, female, and Last names from the 1990 Census report; a
list of names sent to me by Alan Beale; and a few names that I added
myself.  Finally a small list of abbreviations not commonly found in
other word lists is included.

The name files form the Census report is a government document which I
don't think can be copyrighted.

The file special-jargon.50 uses common.lst and word.lst from the
"Unofficial Jargon File Word Lists" which is derived from "The Jargon
File".  All of which is in the Public Domain.  This file also contain
a few extra UNIX terms which are found in the file "unix-terms" in the
special/ directory.

The 55 level includes words from Alan's 2of4brif list found in version
4.0 of his 12dicts package.  Like his other stuff the 2of4brif is also
in the public domain.

The 60 level includes all words appearing in at least 2 of the 12
dictionaries as indicated by the 12Dicts package.

The 70 level includes Brian's frequency class 0 and the 74,550 common
dictionary words from the MWords package.  The common dictionary words,
like those from the 12Dicts package, have had all likely inflections
added.  The 70 level also included the 5desk list from version 4.0 of
the 12Dics package which is the public domain.

The 80 level includes the ENABLE word list, all the lists in the
ENABLE supplement package (except for ABLE), the "UK Advanced Cryptics
Dictionary" (UKACD), the list of signature words in from YAWL package,
and the 10,196 places list from the MWords package.

The ENABLE package, mainted by M\Cooper <thegrendel@theriver.com>,
is in the Public Domain:

  The ENABLE master word list, WORD.LST, is herewith formally released
  into the Public Domain. Anyone is free to use it or distribute it in
  any manner they see fit. No fee or registration is required for its
  use nor are "contributions" solicited (if you feel you absolutely
  must contribute something for your own peace of mind, the authors of
  the ENABLE list ask that you make a donation on their behalf to your
  favorite charity). This word list is our gift to the Scrabble
  community, as an alternate to "official" word lists. Game designers
  may feel free to incorporate the WORD.LST into their games. Please
  mention the source and credit us as originators of the list. Note
  that if you, as a game designer, use the WORD.LST in your product,
  you may still copyright and protect your product, but you may *not*
  legally copyright or in any way restrict redistribution of the
  WORD.LST portion of your product. This *may* under law restrict your
  rights to restrict your users' rights, but that is only fair.

UKACD, by J Ross Beresford <ross@bryson.demon.co.uk>, is under the
following copyright:

  Copyright (c) J Ross Beresford 1993-1999. All Rights Reserved.

  The following restriction is placed on the use of this publication:
  if The UK Advanced Cryptics Dictionary is used in a software package
  or redistributed in any form, the copyright notice must be
  prominently displayed and the text of this document must be included
  verbatim.

  There are no other restrictions: I would like to see the list
  distributed as widely as possible.

The 95 level includes the 354,984 single words, 256,772 compound
words, 4,946 female names and the 3,897 male names, and 21,986 names
from the MWords package, ABLE.LST from the ENABLE Supplement, and some
additional words found in my part-of-speech database that were not
found anywhere else.

Accent information was taken from UKACD.

My VARCON package was used to create the American, British, and
Canadian word list. 

Since the original word lists used used in the VARCON package came
from the Ispell distribution they are under the Ispell copyright:

  Copyright 1993, Geoff Kuenning, Granada Hills, CA
  All rights reserved.

  Redistribution and use in source and binary forms, with or without
  modification, are permitted provided that the following conditions
  are met:

  1. Redistributions of source code must retain the above copyright
     notice, this list of conditions and the following disclaimer.
  2. Redistributions in binary form must reproduce the above copyright
     notice, this list of conditions and the following disclaimer in the
     documentation and/or other materials provided with the distribution.
  3. All modifications to the source code must be clearly marked as
     such.  Binary redistributions based on modified source code
     must be clearly marked as modified versions in the documentation
     and/or other materials provided with the distribution.
  (clause 4 removed with permission from Geoff Kuenning)
  5. The name of Geoff Kuenning may not be used to endorse or promote
     products derived from this software without specific prior
     written permission.

  THIS SOFTWARE IS PROVIDED BY GEOFF KUENNING AND CONTRIBUTORS ``AS
  IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
  LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
  FOR A PARTICULAR PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL GEOFF
  KUENNING OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
  INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
  BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
  LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
  CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
  LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN
  ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
  POSSIBILITY OF SUCH DAMAGE.
```

## Fonts bundled by the user interface library

The app draws text with the Windows system fonts it finds at run time (Arial,
Segoe UI and Tahoma, read from the system font folder; none of them is
shipped with OpenXER). As a fallback for missing faces and characters, the
executable embeds the default fonts of the egui user interface library,
from the `epaint_default_fonts` crate (version 0.33.3, license expression
`(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`). The crate's code is
MIT or Apache-2.0; the fonts it embeds are:

| Font | File | License |
|---|---|---|
| Hack Regular | `Hack-Regular.ttf` | MIT (Hack), public domain (DejaVu changes) and the Bitstream Vera License |
| Noto Emoji Regular | `NotoEmoji-Regular.ttf` | SIL Open Font License 1.1 |
| Ubuntu Light | `Ubuntu-Light.ttf` | Ubuntu Font Licence 1.0 |
| emoji-icon-font | `emoji-icon-font.ttf` | MIT |

The notices and license texts below are copied verbatim from the crate's
`fonts/` folder.

### Hack Regular (`fonts/Hack-Regular.txt`)

```text
The work in the Hack project is Copyright 2018 Source Foundry Authors and licensed under the MIT License

The work in the DejaVu project was committed to the public domain.

Bitstream Vera Sans Mono Copyright 2003 Bitstream Inc. and licensed under the Bitstream Vera License with Reserved Font Names "Bitstream" and "Vera"
MIT License

Copyright (c) 2018 Source Foundry Authors

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
BITSTREAM VERA LICENSE

Copyright (c) 2003 by Bitstream, Inc. All Rights Reserved. Bitstream Vera is a trademark of Bitstream, Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy of the fonts accompanying this license ("Fonts") and associated documentation files (the "Font Software"), to reproduce and distribute the Font Software, including without limitation the rights to use, copy, merge, publish, distribute, and/or sell copies of the Font Software, and to permit persons to whom the Font Software is furnished to do so, subject to the following conditions:

The above copyright and trademark notices and this permission notice shall be included in all copies of one or more of the Font Software typefaces.

The Font Software may be modified, altered, or added to, and in particular the designs of glyphs or characters in the Fonts may be modified and additional glyphs or characters may be added to the Fonts, only if the fonts are renamed to names not containing either the words "Bitstream" or the word "Vera".

This License becomes null and void to the extent applicable to Fonts or Font Software that has been modified and is distributed under the "Bitstream Vera" names.

The Font Software may be sold as part of a larger software package but no copy of one or more of the Font Software typefaces may be sold by itself.

THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL BITSTREAM OR THE GNOME FOUNDATION BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM OTHER DEALINGS IN THE FONT SOFTWARE.

Except as contained in this notice, the names of Gnome, the Gnome Foundation, and Bitstream Inc., shall not be used in advertising or otherwise to promote the sale, use or other dealings in this Font Software without prior written authorization from the Gnome Foundation or Bitstream Inc., respectively. For further information, contact: fonts at gnome dot org.
```

### Noto Emoji Regular: SIL Open Font License 1.1 (`fonts/OFL.txt`)

```text
This Font Software is licensed under the SIL Open Font License,
Version 1.1.

This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL

-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font
creation efforts of academic and linguistic communities, and to
provide a free and open framework in which fonts may be shared and
improved in partnership with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply to
any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software
components as distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to,
deleting, or substituting -- in part or in whole -- any of the
components of the Original Version, by changing formats or by porting
the Font Software to a new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed,
modify, redistribute, and sell modified and unmodified copies of the
Font Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components, in
Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the
corresponding Copyright Holder. This restriction only applies to the
primary font name as presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created using
the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```

### Ubuntu Light: Ubuntu Font Licence 1.0 (`fonts/UFL.txt`)

```text
-------------------------------
UBUNTU FONT LICENCE Version 1.0
-------------------------------

PREAMBLE
This licence allows the licensed fonts to be used, studied, modified and
redistributed freely. The fonts, including any derivative works, can be
bundled, embedded, and redistributed provided the terms of this licence
are met. The fonts and derivatives, however, cannot be released under
any other licence. The requirement for fonts to remain under this
licence does not require any document created using the fonts or their
derivatives to be published under this licence, as long as the primary
purpose of the document is not to be a vehicle for the distribution of
the fonts.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this licence and clearly marked as such. This may
include source files, build scripts and documentation.

"Original Version" refers to the collection of Font Software components
as received under this licence.

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to
a new environment.

"Copyright Holder(s)" refers to all individuals and companies who have a
copyright ownership of the Font Software.

"Substantially Changed" refers to Modified Versions which can be easily
identified as dissimilar to the Font Software by users of the Font
Software comparing the Original Version with the Modified Version.

To "Propagate" a work means to do anything with it that, without
permission, would make you directly or secondarily liable for
infringement under applicable copyright law, except executing it on a
computer or modifying a private copy. Propagation includes copying,
distribution (with or without modification and with or without charging
a redistribution fee), making available to the public, and in some
countries other activities as well.

PERMISSION & CONDITIONS
This licence does not grant any rights under trademark law and all such
rights are reserved.

Permission is hereby granted, free of charge, to any person obtaining a
copy of the Font Software, to propagate the Font Software, subject to
the below conditions:

1) Each copy of the Font Software must contain the above copyright
notice and this licence. These can be included either as stand-alone
text files, human-readable headers or in the appropriate machine-
readable metadata fields within text or binary files as long as those
fields can be easily viewed by the user.

2) The font name complies with the following:
(a) The Original Version must retain its name, unmodified.
(b) Modified Versions which are Substantially Changed must be renamed to
avoid use of the name of the Original Version or similar names entirely.
(c) Modified Versions which are not Substantially Changed must be
renamed to both (i) retain the name of the Original Version and (ii) add
additional naming elements to distinguish the Modified Version from the
Original Version. The name of such Modified Versions must be the name of
the Original Version, with "derivative X" where X represents the name of
the new work, appended to that name.

3) The name(s) of the Copyright Holder(s) and any contributor to the
Font Software shall not be used to promote, endorse or advertise any
Modified Version, except (i) as required by this licence, (ii) to
acknowledge the contribution(s) of the Copyright Holder(s) or (iii) with
their explicit written permission.

4) The Font Software, modified or unmodified, in part or in whole, must
be distributed entirely under this licence, and must not be distributed
under any other licence. The requirement for fonts to remain under this
licence does not affect any document created using the Font Software,
except any version of the Font Software extracted from a document
created using the Font Software may only be distributed under this
licence.

TERMINATION
This licence becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM OTHER
DEALINGS IN THE FONT SOFTWARE.
```

### emoji-icon-font (`fonts/emoji-icon-font-mit-license.txt`)

```text
MIT License

Copyright (c) 2014 John Slegers

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

<!-- BEGIN generated crate notices: tools/licenses/third_party.py -->

## Rust crates

OpenXER and openxer-cli (OpenXER.exe and openxer-cli.exe on Windows) are
built from the 298 crates below: the normal dependencies of
`openxer-desktop` and `openxer-automation` for `x86_64-pc-windows-msvc`
(Windows) and `x86_64-unknown-linux-gnu` (Linux), from `Cargo.lock`. A crate
that only one system's executables contain is marked with that system (10
marked "Windows only", 101 marked "Linux only"); the rest are in both. Each
is listed under the license expression its `Cargo.toml` declares, with the
copyright lines of its license files. The full license texts follow the
list; files that are not a plain MIT or Apache-2.0 text, and every NOTICE
file, are reproduced per crate after that. En and em dashes in reproduced
texts are written as hyphens.
Regenerate this part with `python tools/licenses/third_party.py`.

## SQLite

OpenXER and openxer-cli include SQLite 3.50.2, compiled from the
amalgamation bundled in the `libsqlite3-sys` 0.35.0 crate
(listed below with its own license). SQLite is in the public domain; its
source carries this blessing in place of a license:

```text
The author disclaims copyright to this source code.  In place of
a legal notice, here is a blessing:

   May you do good and not evil.
   May you find forgiveness for yourself and forgive others.
   May you share freely, never taking more than you give.
```

### Crates by license

#### MIT OR Apache-2.0 (136 crates)

- `aes` 0.8.4: Copyright (c) 2018 Artyom Pavlov
- `ahash` 0.8.12: Copyright (c) 2018 Tom Kaitchuck
- `arboard` 3.7.0: Copyright (c) 2022 The Arboard contributors
- `as-raw-xcb-connection` 1.0.1 (Linux only): Copyright 2019 as-raw-xcb-connection Contributers
- `async-broadcast` 0.7.2 (Linux only): Copyright 2020 Yoshua Wuyts; Copyright (c) 2020 Yoshua Wuyts
- `async-recursion` 1.2.0 (Linux only): no copyright line in the package; authors: Robert Usher <266585+dcchut@users.noreply.github.com>
- `async-trait` 0.1.92 (Linux only): no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `atoi_simd` 0.18.1: Copyright 2022-NOW Dmitry Rodionov; Copyright (c) 2022 Dmitry Rodionov
- `bitflags` 2.13.2: Copyright (c) 2014 The Rust Project Developers
- `block-buffer` 0.10.4: Copyright (c) 2018-2019 The RustCrypto Project Developers
- `block-padding` 0.3.3: Copyright (c) 2018-2019 The RustCrypto Project Developers
- `bumpalo` 3.20.3: Copyright (c) 2019 Nick Fitzgerald
- `cbc` 0.1.2: Copyright (c) 2018-2022 RustCrypto Developers; Copyright (c) 2018 Artyom Pavlov
- `cff-parser` 0.2.0: Copyright (c) 2018 Yevhenii Reizner
- `cfg-if` 1.0.5: Copyright (c) 2014 Alex Crichton
- `chacha20` 0.10.2: Copyright (c) 2019-2026 The RustCrypto Project Developers
- `chrono` 0.4.45: Copyright (c) 2014, Kang Seonghoon.
- `cipher` 0.4.4: Copyright (c) 2016-2020 RustCrypto Developers
- `cpufeatures` 0.2.17: Copyright (c) 2020-2025 The RustCrypto Project Developers
- `cpufeatures` 0.3.1: Copyright (c) 2020-2026 The RustCrypto Project Developers
- `crc32fast` 1.5.4: Copyright (c) 2018 Sam Rijs, Alex Crichton and contributors
- `crossbeam-utils` 0.8.23 (Linux only): Copyright (c) 2019 The Crossbeam Project Developers
- `crypto-common` 0.1.7: Copyright (c) 2021 RustCrypto Developers
- `debug_unsafe` 0.1.4: Copyright 2025-NOW Dmitry Rodionov; Copyright (c) 2025 Dmitry Rodionov
- `digest` 0.10.7: Copyright (c) 2017 Artyom Pavlov
- `displaydoc` 0.2.7: no copyright line in the package; authors: Jane Lusby <jlusby@yaah.dev>
- `document-features` 0.2.12: Copyright (c) 2020 Olivier Goffart <ogoffart@sixtyfps.io>
- `ecolor` 0.33.3: no copyright line in the package; authors: Emil Ernerfeldt <emil.ernerfeldt@gmail.com>, Andreas Reich <reichandreas@gmx.de>
- `eframe` 0.33.3: no copyright line in the package; authors: Emil Ernerfeldt <emil.ernerfeldt@gmail.com>
- `egui` 0.33.3: no copyright line in the package; authors: Emil Ernerfeldt <emil.ernerfeldt@gmail.com>
- `egui-winit` 0.33.3: no copyright line in the package; authors: Emil Ernerfeldt <emil.ernerfeldt@gmail.com>
- `egui_extras` 0.33.3: no copyright line in the package; authors: Dominik Rössler <dominik@freshx.de>, Emil Ernerfeldt <emil.ernerfeldt@gmail.com>, René Rössler <rene@freshx.de>
- `egui_glow` 0.33.3: no copyright line in the package; authors: Emil Ernerfeldt <emil.ernerfeldt@gmail.com>
- `emath` 0.33.3: no copyright line in the package; authors: Emil Ernerfeldt <emil.ernerfeldt@gmail.com>
- `enum-map` 2.7.3: no copyright line in the package; authors: Kamila Borowska <kamila@borowska.pw>
- `enum-map-derive` 0.17.0: no copyright line in the package; authors: Kamila Borowska <kamila@borowska.pw>
- `enumflags2` 0.7.12 (Linux only): Copyright 2017-2023 Maik Klein, Maja Kądziołka; Copyright (c) 2017-2023 Maik Klein, Maja Kądziołka
- `enumflags2_derive` 0.7.12 (Linux only): Copyright (c) 2017 Maik Klein
- `epaint` 0.33.3: no copyright line in the package; authors: Emil Ernerfeldt <emil.ernerfeldt@gmail.com>
- `errno` 0.3.14 (Linux only): Copyright (c) 2014 Chris Wong
- `fast-float2` 0.2.4: Copyright 2021 Ivan Smirnov
- `fdeflate` 0.3.7: no copyright line in the package; authors: The image-rs Developers
- `flate2` 1.1.10: Copyright (c) 2014-2026 Alex Crichton
- `form_urlencoded` 1.2.2: Copyright (c) 2013-2016 The rust-url developers
- `futures-channel` 0.3.34 (Linux only): Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors
- `futures-core` 0.3.34 (Linux only): Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors
- `futures-io` 0.3.34 (Linux only): Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors
- `futures-macro` 0.3.34 (Linux only): Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors
- `futures-task` 0.3.34 (Linux only): Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors
- `futures-util` 0.3.34 (Linux only): Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors
- `getrandom` 0.3.4 (Linux only): Copyright (c) 2018-2025 The rust-random Project Developers; Copyright (c) 2014 The Rust Project Developers
- `getrandom` 0.4.3: Copyright (c) 2018-2026 The rust-random Project Developers; Copyright (c) 2014 The Rust Project Developers
- `hashbrown` 0.15.5: Copyright (c) 2016 Amanieu d'Antras
- `hashbrown` 0.17.1: Copyright (c) 2016 Amanieu d'Antras
- `hashlink` 0.10.0: no copyright line in the package; authors: kyren <kerriganw@gmail.com>
- `hex` 0.4.3 (Linux only): Copyright (c) 2013-2014 The Rust Project Developers.; Copyright (c) 2015-2020 The rust-hex Developers
- `iana-time-zone` 0.1.65 (Linux only): Copyright 2020 Andrew Straw; Copyright (c) 2020 Andrew D. Straw
- `idna` 1.1.0: Copyright (c) 2013-2025 The rust-url developers
- `image` 0.25.10: no copyright line in the package; authors: The image-rs Developers
- `inout` 0.1.4: Copyright (c) 2022 The RustCrypto Project Developers; Copyright (c) 2022 Artyom Pavlov
- `itoa` 1.0.18: no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `libc` 0.2.191 (Linux only): Copyright (c) The Rust Project Developers
- `litrs` 1.0.1: Copyright (c) 2020 Project Developers
- `lock_api` 0.4.14: Copyright (c) 2016 The Rust Project Developers
- `log` 0.4.35: Copyright (c) 2014 The Rust Project Developers
- `md-5` 0.10.6: Copyright (c) 2006-2009 Graydon Hoare; Copyright (c) 2009-2013 Mozilla Foundation; Copyright (c) 2016 Artyom Pavlov
- `memmap2` 0.9.12 (Linux only): Copyright (c) 2020 Yevhenii Reizner; Copyright (c) 2015 Dan Burkert
- `mime` 0.3.17: Copyright (c) 2014 Sean McArthur
- `native-tls` 0.2.18: Copyright (c) 2016 The rust-native-tls Developers
- `num-bigint` 0.4.8: Copyright (c) 2014 The Rust Project Developers
- `num-integer` 0.1.47: Copyright (c) 2014 The Rust Project Developers
- `num-rational` 0.4.2: Copyright (c) 2014 The Rust Project Developers
- `num-traits` 0.2.19: Copyright (c) 2014 The Rust Project Developers
- `once_cell` 1.21.4: no copyright line in the package; authors: Aleksey Kladov <aleksey.kladov@gmail.com>
- `openssl-probe` 0.2.2 (Linux only): Copyright (c) 2014 Alex Crichton
- `ordered-stream` 0.2.0 (Linux only): no copyright line in the package; authors: Daniel De Graaf <code@danieldg.net>, Zeeshan Ali Khan <zeeshanak@gnome.org>
- `parking_lot` 0.12.5: Copyright (c) 2016 The Rust Project Developers
- `parking_lot_core` 0.9.12: Copyright (c) 2016 The Rust Project Developers
- `pdf-writer` 0.15.0: no copyright line in the package; authors: Laurenz <laurmaedje@gmail.com>, Martin Haug <mhaug@live.de>
- `percent-encoding` 2.3.2: Copyright (c) 2013-2025 The rust-url developers
- `piper` 0.2.5 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>, John Nunley <dev@notgull.net>
- `png` 0.18.1: Copyright (c) 2015 nwin
- `ppv-lite86` 0.2.21 (Linux only): Copyright 2019 The CryptoCorrosion Contributors; Copyright (c) 2019 The CryptoCorrosion Contributors
- `proc-macro-crate` 3.5.0 (Linux only): no copyright line in the package; authors: Bastian Köcher <git@kchr.de>
- `proc-macro2` 1.0.107: no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>, Alex Crichton <alex@alexcrichton.com>
- `profiling` 1.0.18: no copyright line in the package; authors: Philip Degarmo <aclysma@gmail.com>
- `quote` 1.0.47: no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `rand` 0.10.3: Copyright 2018 Developers of the Rand project; Copyright (c) 2014 The Rust Project Developers
- `rand` 0.9.5 (Linux only): Copyright 2018 Developers of the Rand project; Copyright (c) 2014 The Rust Project Developers
- `rand_chacha` 0.9.0 (Linux only): Copyright 2018 Developers of the Rand project; Copyright (c) 2014 The Rust Project Developers
- `rand_core` 0.10.1: Copyright (c) 2018-2026 The Rand Project Developers
- `rand_core` 0.9.5 (Linux only): Copyright 2018 Developers of the Rand project; Copyright (c) 2014 The Rust Project Developers
- `rust_xlsxwriter` 0.102.0: Copyright 2022-2026 John McNamara <jmcnamara@cpan.org>
- `rustversion` 1.0.23: no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `scopeguard` 1.2.0: Copyright (c) 2016-2019 Ulrik Sverdrup "bluss" and scopeguard developers
- `serde` 1.0.229: no copyright line in the package; authors: Erick Tryzelaar <erick.tryzelaar@gmail.com>, David Tolnay <dtolnay@gmail.com>
- `serde_core` 1.0.229: no copyright line in the package; authors: Erick Tryzelaar <erick.tryzelaar@gmail.com>, David Tolnay <dtolnay@gmail.com>
- `serde_derive` 1.0.229: no copyright line in the package; authors: Erick Tryzelaar <erick.tryzelaar@gmail.com>, David Tolnay <dtolnay@gmail.com>
- `serde_json` 1.0.154: no copyright line in the package; authors: the serde_json developers
- `serde_repr` 0.1.21 (Linux only): no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `sha2` 0.10.9: Copyright (c) 2006-2009 Graydon Hoare; Copyright (c) 2009-2013 Mozilla Foundation; Copyright (c) 2016 Artyom Pavlov
- `signal-hook-registry` 1.4.8 (Linux only): Copyright (c) 2017 tokio-jsonrpc developers
- `simdutf8` 0.1.5: no copyright line in the package; authors: Hans Kratz <hans@appfour.com>
- `smallvec` 1.16.4: Copyright (c) 2018 The Servo Project Developers
- `smol_str` 0.2.2: no copyright line in the package; authors: Aleksey Kladov <aleksey.kladov@gmail.com>
- `stable_deref_trait` 1.2.1: Copyright (c) 2017 Robert Grosse
- `static_assertions` 1.1.0: Copyright (c) 2017 Nikolai Vazquez
- `syn` 2.0.119: no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `syn` 3.0.9: no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `tempfile` 3.27.0: Copyright (c) 2015 Steven Allen
- `thiserror` 1.0.69 (Linux only): no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `thiserror` 2.0.21: no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `thiserror-impl` 1.0.69 (Linux only): no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `thiserror-impl` 2.0.21: no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `toml_datetime` 1.1.2+spec-1.1.0: Copyright Individual contributors; Copyright (c) Individual contributors
- `toml_edit` 0.25.18+spec-1.1.0: Copyright Individual contributors; Copyright (c) Individual contributors
- `toml_parser` 1.1.5+spec-1.1.0: Copyright Individual contributors; Copyright (c) Individual contributors
- `toml_writer` 1.2.0+spec-1.1.0: Copyright Individual contributors; Copyright (c) Individual contributors
- `ttf-parser` 0.25.1: Copyright (c) 2018 Yevhenii Reizner
- `typed-path` 0.13.0: no copyright line in the package; authors: Chip Senkbeil <chip@senkbeil.org>
- `typenum` 1.20.1: Copyright 2014 Paho Lurie-Gregg; Copyright (c) 2014 Paho Lurie-Gregg
- `unicase` 2.10.0: Copyright (c) 2014-2026 Sean McArthur
- `unicode-bidi` 0.3.18: Copyright (c) 2015 The Rust Project Developers
- `unicode-segmentation` 1.13.3: Copyright (c) 2015 The Rust Project Developers
- `url` 2.5.8: Copyright (c) 2013-2025 The rust-url developers
- `web-time` 1.1.0: Copyright 2023 dAxpeDDa; Copyright (c) 2023 dAxpeDDa
- `webbrowser` 1.2.6: Copyright (c) 2015-2022 Amod Malviya
- `weezl` 0.1.12: Copyright (c) HeroicKatora 2020
- `windows-link` 0.2.1 (Windows only): Copyright (c) Microsoft Corporation.
- `windows-sys` 0.52.0 (Windows only): Copyright (c) Microsoft Corporation.
- `windows-sys` 0.59.0 (Windows only): Copyright (c) Microsoft Corporation.
- `windows-sys` 0.61.2 (Windows only): Copyright (c) Microsoft Corporation.
- `windows-targets` 0.52.6 (Windows only): Copyright (c) Microsoft Corporation.
- `windows_x86_64_msvc` 0.52.6 (Windows only): Copyright (c) Microsoft Corporation.
- `x11rb` 0.13.2 (Linux only): Copyright 2019 x11rb Contributers
- `x11rb-protocol` 0.13.2 (Linux only): Copyright 2019 x11rb Contributers

#### MIT (60 crates)

- `adobe-cmap-parser` 0.4.1: no copyright line in the package; authors: Jeff Muizelaar <jmuizelaar@mozilla.com>
- `ashpd` 0.11.1 (Linux only): Copyright (c) 2020 Bilal Elmoussaoui
- `calamine` 0.38.0: Copyright (c) 2016 Johann Tuffe
- `calloop` 0.13.0 (Linux only): Copyright (c) 2018 Victor Berger
- `calloop` 0.14.5 (Linux only): Copyright (c) 2018 Victor Berger
- `calloop-wayland-source` 0.3.0 (Linux only): Copyright (c) 2023 Kirill Chibisov
- `calloop-wayland-source` 0.4.1 (Linux only): Copyright (c) 2023 Kirill Chibisov
- `dlib` 0.5.3 (Linux only): Copyright (c) 2015 Victor Berger
- `ecb` 0.1.2: Copyright (c) magic-akari
- `endi` 1.1.1 (Linux only): no copyright line in the package; authors: Zeeshan Ali Khan <zeenix@gmail.com>
- `generic-array` 0.14.7: Copyright (c) 2015 Bartłomiej Kamiński
- `glutin-winit` 0.5.0: Copyright © 2022 Kirill Chibisov
- `libsqlite3-sys` 0.35.0: Copyright (c) 2014 The rusqlite developers
- `lopdf` 0.42.0: Copyright (c) 2016 Junfeng Liu
- `memoffset` 0.9.1: Copyright (c) 2017 Gilad Naaman
- `mime_guess2` 2.3.1: Copyright (c) 2015 Austin Bonander
- `nom` 8.0.0: Copyright (c) 2014-2019 Geoffroy Couprie
- `openssl-sys` 0.9.118 (Linux only): Copyright (c) 2014 Alex Crichton
- `pdf-extract` 0.12.1: no copyright line in the package; authors: Jeff Muizelaar <jmuizelaar@mozilla.com>
- `pom` 1.1.0: Copyright (c) 2016 Junfeng Liu
- `quick-xml` 0.41.0 (Linux only): Copyright (c) 2016 Johann Tuffe
- `quick-xml` 0.42.0: Copyright (c) 2016 Johann Tuffe
- `rfd` 0.15.4: Copyright (c) 2022 Bartłomiej Maryńczak
- `rusqlite` 0.37.0: Copyright (c) 2014 The rusqlite developers
- `schannel` 0.1.30 (Windows only): Copyright (c) 2015 steffengy
- `simd-adler32` 0.3.10: Copyright (c) [2021] [Marvin Countryman]
- `slab` 0.4.12 (Linux only): Copyright (c) 2019 Carl Lerche
- `smithay-client-toolkit` 0.19.2 (Linux only): Copyright (c) 2018 Victor Berger
- `smithay-client-toolkit` 0.20.0 (Linux only): Copyright (c) 2018 Victor Berger
- `smithay-clipboard` 0.7.3 (Linux only): Copyright (c) 2018 Lucas Timmins & Victor Berger
- `synstructure` 0.14.0: Copyright 2016 Nika Layzell
- `tracing` 0.1.44: Copyright (c) 2019 Tokio Contributors
- `tracing-attributes` 0.1.31 (Linux only): Copyright (c) 2019 Tokio Contributors
- `tracing-core` 0.1.36: Copyright (c) 2019 Tokio Contributors
- `type1-encoding-parser` 0.1.1: no copyright line in the package; authors: Jeff Muizelaar <jmuizelaar@mozilla.com>
- `urlencoding` 2.1.3 (Linux only): © 2016 Bertram Truong; © 2021 Kornel Lesiński
- `wayland-backend` 0.3.17 (Linux only): Copyright (c) 2015 Elinor Berger
- `wayland-client` 0.31.15 (Linux only): Copyright (c) 2015 Elinor Berger
- `wayland-csd-frame` 0.3.0 (Linux only): Copyright (c) 2023 Kirill Chibisov
- `wayland-cursor` 0.31.14 (Linux only): Copyright (c) 2015 Elinor Berger
- `wayland-protocols` 0.32.13 (Linux only): Copyright (c) 2015 Elinor Berger
- `wayland-protocols-experimental` 20250721.0.1 (Linux only): Copyright (c) 2015 Elinor Berger
- `wayland-protocols-misc` 0.3.12 (Linux only): Copyright (c) 2015 Elinor Berger
- `wayland-protocols-plasma` 0.3.12 (Linux only): Copyright (c) 2015 Elinor Berger
- `wayland-protocols-wlr` 0.3.12 (Linux only): Copyright (c) 2015 Elinor Berger
- `wayland-scanner` 0.31.11 (Linux only): Copyright (c) 2015 Elinor Berger
- `wayland-sys` 0.31.11 (Linux only): Copyright (c) 2015 Elinor Berger
- `winnow` 1.0.4: no copyright line in the package; authors: the winnow developers
- `x11-dl` 2.21.0 (Linux only): no copyright line in the package; authors: daggerbot <daggerbot@gmail.com>, Erle Pereira <erle@erlepereira.com>, AltF02 <contact@altf2.dev>
- `xcursor` 0.3.11 (Linux only): Copyright (c) 2020 Samuele Esposito
- `xkbcommon-dl` 0.4.2 (Linux only): Copyright (c) 2023 Kirill Chibisov
- `zbus` 5.19.0 (Linux only): Copyright (c) 2024 Zeeshan Ali Khan & zbus contributors
- `zbus_macros` 5.19.0 (Linux only): Copyright (c) 2024 Zeeshan Ali Khan & zbus contributors
- `zbus_names` 4.3.5 (Linux only): Copyright (c) 2024 Zeeshan Ali Khan & zbus contributors
- `zcheapstr` 1.2.0 (Linux only): Copyright (c) 2026 Zeeshan Ali Khan & zcheapstr contributors
- `zip` 9.3.0: Copyright (c) 2014 Mathijs van de Nes
- `zmij` 1.0.23: no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>
- `zvariant` 5.15.1 (Linux only): Copyright (c) 2024 Zeeshan Ali Khan & zbus contributors
- `zvariant_derive` 5.15.1 (Linux only): Copyright (c) 2024 Zeeshan Ali Khan & zbus contributors
- `zvariant_utils` 4.2.0 (Linux only): no copyright line in the package; authors: Zeeshan Ali Khan <zeeshanak@gnome.org>, turbocooler <turbocooler@cocaine.ninja>

#### Apache-2.0 OR MIT (28 crates)

- `async-channel` 2.5.0 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>
- `async-executor` 1.14.1 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>, John Nunley <dev@notgull.net>
- `async-fs` 2.2.0 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>
- `async-io` 2.6.0 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>
- `async-lock` 3.5.0 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>
- `async-net` 2.0.0 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>
- `async-process` 2.5.0 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>
- `async-signal` 0.2.14 (Linux only): no copyright line in the package; authors: John Nunley <dev@notgull.net>
- `async-task` 4.7.1 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>
- `atomic-waker` 1.1.2 (Linux only): Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors
- `blocking` 1.7.0 (Linux only): no copyright line in the package; authors: the blocking developers
- `codepage` 0.1.3: Copyright Mozilla Foundation
- `concurrent-queue` 2.5.0 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>, Taiki Endo <te316e89@gmail.com>, John Nunley <dev@notgull.net>
- `equivalent` 1.0.3: Copyright (c) 2016--2023
- `event-listener` 5.4.2 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>, John Nunley <dev@notgull.net>
- `event-listener-strategy` 0.5.4 (Linux only): no copyright line in the package; authors: John Nunley <dev@notgull.net>
- `fastrand` 2.5.0: no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>
- `futures-lite` 2.6.1 (Linux only): Copyright (c) 2016 Alex Crichton; Copyright (c) 2017 The Tokio Authors
- `idna_adapter` 1.2.2: Copyright (c) The rust-url developers
- `indexmap` 2.14.2: Copyright (c) 2016--2017
- `multiversion_no_op` 1.0.0: Copyright Mozilla Foundation
- `nohash-hasher` 0.2.0: Copyright 2018 Parity Technologies (UK) Ltd.
- `parking` 2.2.1 (Linux only): Copyright 2014-2020 The Rust Project Developers
- `pin-project-lite` 0.2.17: no copyright line in the package; authors: the pin-project-lite developers
- `polling` 3.11.0 (Linux only): no copyright line in the package; authors: Stjepan Glavina <stjepang@gmail.com>, John Nunley <dev@notgull.net>
- `utf8_iter` 1.1.0: Copyright Mozilla Foundation
- `utf_types` 1.0.0: Copyright Mozilla Foundation
- `uuid` 1.29.0 (Linux only): Copyright (c) 2014 The Rust Project Developers; Copyright (c) 2018 Ashley Mannix, Christopher Armstrong, Dylan DPC, Hunar Roop Kahlon

#### Unicode-3.0 (18 crates)

- `icu_collections` 2.4.0: Copyright © 2020-2024 Unicode, Inc.
- `icu_locale_core` 2.4.0: Copyright © 2020-2024 Unicode, Inc.
- `icu_normalizer` 2.4.0: Copyright © 2020-2024 Unicode, Inc.
- `icu_normalizer_data` 2.4.0: Copyright © 2020-2024 Unicode, Inc.
- `icu_properties` 2.4.0: Copyright © 2020-2024 Unicode, Inc.
- `icu_properties_data` 2.4.0: Copyright © 2020-2024 Unicode, Inc.
- `icu_provider` 2.4.0: Copyright © 2020-2024 Unicode, Inc.
- `litemap` 0.8.3: Copyright © 2020-2024 Unicode, Inc.
- `potential_utf` 0.1.6: Copyright © 2020-2024 Unicode, Inc.
- `tinystr` 0.8.5: Copyright © 2020-2024 Unicode, Inc.
- `writeable` 0.6.4: Copyright © 2020-2024 Unicode, Inc.
- `yoke` 0.8.4: Copyright © 2020-2024 Unicode, Inc.
- `yoke-derive` 0.8.4: Copyright © 2020-2024 Unicode, Inc.
- `zerofrom` 0.1.8: Copyright © 2020-2024 Unicode, Inc.
- `zerofrom-derive` 0.1.8: Copyright © 2020-2024 Unicode, Inc.
- `zerotrie` 0.2.5: Copyright © 2020-2024 Unicode, Inc.
- `zerovec` 0.11.9: Copyright © 2020-2024 Unicode, Inc.
- `zerovec-derive` 0.11.7: Copyright © 2020-2024 Unicode, Inc.

#### Apache-2.0 (11 crates)

- `ab_glyph` 0.2.32: no copyright line in the package; authors: Alex Butler <alexheretic@gmail.com>
- `ab_glyph_rasterizer` 0.1.10: no copyright line in the package; authors: Alex Butler <alexheretic@gmail.com>
- `gethostname` 1.1.0 (Linux only): no copyright line in the package; authors: Sebastian Wiesner <sebastian@swsnr.de>
- `glutin` 0.32.3: Copyright 2022 Kirill Chibisov
- `glutin_egl_sys` 0.7.1: Copyright 2022 Kirill Chibisov
- `glutin_glx_sys` 0.6.1 (Linux only): Copyright 2022 Kirill Chibisov
- `glutin_wgl_sys` 0.6.1 (Windows only): Copyright 2022 Kirill Chibisov
- `openssl` 0.10.82 (Linux only): Copyright 2011-2017 Google Inc.
- `owned_ttf_parser` 0.25.1: no copyright line in the package; authors: Alex Butler <alexheretic@gmail.com>
- `winit` 0.30.13: no copyright line in the package; authors: The winit contributors, Pierre Krieger <pierre.krieger1708@gmail.com>
- `zopfli` 0.8.4: Copyright 2011 Google Inc.

#### MIT/Apache-2.0 (10 crates)

- `core_detect` 1.0.0: Copyright (c) 2017-2020 The Rust Project Developers
- `downcast-rs` 1.2.1 (Linux only): Copyright (c) 2020 Ashish Myles and contributors
- `fallible-iterator` 0.3.0: Copyright (c) 2015 The rust-openssl-verify Developers
- `fallible-streaming-iterator` 0.1.9: Copyright (c) 2016 The fallible-streaming-iterator Developers
- `foreign-types` 0.3.2 (Linux only): Copyright (c) 2017 The foreign-types Developers
- `foreign-types-shared` 0.1.1 (Linux only): Copyright (c) 2017 The foreign-types Developers
- `openssl-macros` 0.1.1 (Linux only): Copyright (c) 2022 Steven Fackler
- `rangemap` 1.8.0: Copyright 2019-2022 Jeff Parsons, and [contributors](https://github.com/jeffparsons/rangemap/contributors); Copyright 2019 Jeffrey Parsons
- `scoped-tls` 1.0.1 (Linux only): Copyright (c) 2014 Alex Crichton
- `stringprep` 0.1.5: Copyright (c) 2017 The rust-stringprep Developers

#### Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT (4 crates)

- `linux-raw-sys` 0.12.1 (Linux only): no copyright line in the package; authors: Dan Gohman <dev@sunfishcode.online>
- `linux-raw-sys` 0.4.15 (Linux only): no copyright line in the package; authors: Dan Gohman <dev@sunfishcode.online>
- `rustix` 0.38.44 (Linux only): no copyright line in the package; authors: Dan Gohman <dev@sunfishcode.online>, Jakub Konka <kubkon@jakubkonka.com>
- `rustix` 1.1.6 (Linux only): no copyright line in the package; authors: Dan Gohman <dev@sunfishcode.online>, Jakub Konka <kubkon@jakubkonka.com>

#### MIT OR Apache-2.0 OR Zlib (4 crates)

- `cursor-icon` 1.2.0: Copyright 2023 Kirill Chibisov; Copyright (c) 2023 Kirill Chibisov
- `glow` 0.16.0: no copyright line in the package; authors: Joshua Groves <josh@joshgroves.com>, Dzmitry Malyshau <kvarkus@gmail.com>
- `raw-window-handle` 0.6.2: Copyright (c) 2019 Osspial; Copyright (c) 2020 Osspial
- `xkeysym` 0.2.1 (Linux only): Copyright 2022-2023 John Nunley; Copyright (c) 2022-2023 John Nunley

#### (MIT OR Apache-2.0) AND Unicode-3.0 (3 crates)

- `unicode-ident` 1.0.27: Copyright © 1991-2023 Unicode, Inc.
- `unicode-normalization` 0.1.26: Copyright (c) 2015 The Rust Project Developers; Copyright © 1991-2023 Unicode, Inc.
- `unicode-properties` 0.1.5: Copyright (c) 2015 The Rust Project Developers; Copyright © 1991-2026 Unicode, Inc.

#### Unlicense OR MIT (3 crates)

- `byteorder` 1.5.0: Copyright (c) 2015 Andrew Gallant
- `byteorder-lite` 0.1.0: Copyright (c) 2015 Andrew Gallant
- `memchr` 2.8.3: Copyright (c) 2015 Andrew Gallant

#### Zlib OR Apache-2.0 OR MIT (3 crates)

- `bytemuck` 1.25.2: Copyright (c) 2019 Daniel "Lokathor" Gee.
- `bytemuck_derive` 1.12.1: Copyright (c) 2019 Daniel "Lokathor" Gee.
- `tinyvec` 1.13.3: Copyright (c) 2019 Daniel "Lokathor" Gee.

#### Apache-2.0/MIT (2 crates)

- `pollster` 0.4.0 (Linux only): Copyright (c) 2020-2021 Joshua Barretto
- `postscript` 0.14.1: Copyright 2015-2022 The postscript Developers

#### BSD-3-Clause OR Apache-2.0 (2 crates)

- `moxcms` 0.8.1: Copyright 2024 Radzivon Bartoshyk; Copyright (c) Radzivon Bartoshyk. All rights reserved.
- `pxfm` 0.1.30: Copyright 2024 Radzivon Bartoshyk; Copyright (c) Radzivon Bartoshyk. All rights reserved.

#### BSL-1.0 (2 crates)

- `clipboard-win` 5.4.3 (Windows only): no copyright line in the package; authors: Douman <douman@gmx.se>
- `error-code` 3.4.0 (Windows only): no copyright line in the package; authors: Douman <douman@gmx.se>

#### MIT OR Zlib OR Apache-2.0 (2 crates)

- `miniz_oxide` 0.8.9: Copyright 2013-2014 RAD Game Tools and Valve Software; Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC; Copyright (c) 2017 Frommi; Copyright (c) 2017-2024 oyvindln; Copyright (c) 2020 Frommi
- `miniz_oxide` 0.9.1: Copyright 2013-2014 RAD Game Tools and Valve Software; Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC; Copyright (c) 2017 Frommi; Copyright (c) 2017-2024 oyvindln; Copyright (c) 2020 Frommi

#### Zlib (2 crates)

- `foldhash` 0.1.5: Copyright (c) 2024 Orson Peters
- `zlib-rs` 0.6.8: (C) 2024 Trifecta Tech Foundation

#### (Apache-2.0 OR MIT) AND BSD-3-Clause (1 crate)

- `encoding_rs` 0.8.44: Copyright Mozilla Foundation; Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).

#### (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0 (1 crate)

- `epaint_default_fonts` 0.33.3: no copyright line in the package; authors: Emil Ernerfeldt <emil.ernerfeldt@gmail.com>

#### 0BSD OR MIT OR Apache-2.0 (1 crate)

- `adler2` 2.0.1: Copyright (C) Jonas Schievink <jonasschievink@gmail.com>

#### Apache-2.0 AND MIT (1 crate)

- `dpi` 0.1.2: Copyright (c) 2018 Jorge Aparicio; Copyright © 2005-2020 Rich Felker, et al.; Copyright © 1993,2004 Sun Microsystems or; Copyright © 2003-2011 David Schultz or; Copyright © 2003-2009 Steven G. Kargl or; Copyright © 2003-2009 Bruce D. Evans or; Copyright © 2008 Stephen L. Moshier or; Copyright © 2017-2018 Arm Limited

#### Apache-2.0 OR BSL-1.0 (1 crate)

- `ryu` 1.0.23: no copyright line in the package; authors: David Tolnay <dtolnay@gmail.com>

#### BSD-2-Clause OR Apache-2.0 OR MIT (1 crate)

- `zerocopy` 0.8.63: Copyright 2023 The Fuchsia Authors; Copyright 2019 The Fuchsia Authors.

#### ISC (1 crate)

- `libloading` 0.8.9: Copyright © 2015, Simonas Kazlauskas

#### MIT / Apache-2.0 (1 crate)

- `euclid` 0.20.14: Copyright (c) 2012-2013 Mozilla Foundation

### License texts

#### Apache-2.0

Each crate under this license is covered by this text with its own
copyright lines above.

```text
                              Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

   http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### MIT

Each crate under this license is covered by this text with its own
copyright lines above.

```text
Permission is hereby granted, free of charge, to any
person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without
limitation the rights to use, copy, modify, merge,
publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software
is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice
shall be included in all copies or substantial portions
of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF
ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED
TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A
PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT
SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR
IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

#### Unicode-3.0

Each crate under this license is covered by this text with its own
copyright lines above.

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE


NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

-

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### License and notice files reproduced per crate

#### `adler2` 2.0.1: `LICENSE-0BSD`

```text
Copyright (C) Jonas Schievink <jonasschievink@gmail.com>

Permission to use, copy, modify, and/or distribute this software for
any purpose with or without fee is hereby granted.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN
AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT
OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

#### `bytemuck` 1.25.2: `LICENSE-ZLIB`

```text
Copyright (c) 2019 Daniel "Lokathor" Gee.

This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### `bytemuck_derive` 1.12.1: `LICENSE-ZLIB`

```text
Copyright (c) 2019 Daniel "Lokathor" Gee.

This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### `byteorder` 1.5.0: `COPYING`

```text
This project is dual-licensed under the Unlicense and MIT licenses.

You may use this code under the terms of either license.
```

#### `byteorder` 1.5.0: `UNLICENSE`

```text
This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or
distribute this software, either in source code form or as a compiled
binary, for any purpose, commercial or non-commercial, and by any
means.

In jurisdictions that recognize copyright laws, the author or authors
of this software dedicate any and all copyright interest in the
software to the public domain. We make this dedication for the benefit
of the public at large and to the detriment of our heirs and
successors. We intend this dedication to be an overt act of
relinquishment in perpetuity of all present and future rights to this
software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <http://unlicense.org/>
```

#### `byteorder-lite` 0.1.0: `UNLICENSE`

```text
This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or
distribute this software, either in source code form or as a compiled
binary, for any purpose, commercial or non-commercial, and by any
means.

In jurisdictions that recognize copyright laws, the author or authors
of this software dedicate any and all copyright interest in the
software to the public domain. We make this dedication for the benefit
of the public at large and to the detriment of our heirs and
successors. We intend this dedication to be an overt act of
relinquishment in perpetuity of all present and future rights to this
software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <http://unlicense.org/>
```

#### `chrono` 0.4.45: `LICENSE.txt`

```text
Rust-chrono is dual-licensed under The MIT License [1] and
Apache 2.0 License [2]. Copyright (c) 2014--2026, Kang Seonghoon and
contributors.

Nota Bene: This is same as the Rust Project's own license.


[1]: <http://opensource.org/licenses/MIT>, which is reproduced below:

~~~~
The MIT License (MIT)

Copyright (c) 2014, Kang Seonghoon.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
~~~~


[2]: <http://www.apache.org/licenses/LICENSE-2.0>, which is reproduced below:

~~~~
                              Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS

APPENDIX: How to apply the Apache License to your work.

   To apply the Apache License to your work, attach the following
   boilerplate notice, with the fields enclosed by brackets "[]"
   replaced with your own identifying information. (Don't include
   the brackets!)  The text should be enclosed in the appropriate
   comment syntax for the file format. We also recommend that a
   file or class name and description of purpose be included on the
   same "printed page" as the copyright notice for easier
   identification within third-party archives.

Copyright [yyyy] [name of copyright owner]

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

	http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
~~~~
```

#### `codepage` 0.1.3: `COPYRIGHT`

```text
codepage is copyright 2018 Mozilla Foundation.

Licensed under the Apache License, Version 2.0
<LICENSE-APACHE or
https://www.apache.org/licenses/LICENSE-2.0> or the MIT
license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
at your option. All files in the project carrying such
notice may not be copied, modified, or distributed except
according to those terms.
```

#### `cursor-icon` 1.2.0: `LICENSE-ZLIB`

```text
Copyright (c) 2023 Kirill Chibisov

This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### `encoding_rs` 0.8.44: `COPYRIGHT`

```text
encoding_rs is copyright Mozilla Foundation.

Licensed under the Apache License, Version 2.0
<LICENSE-APACHE or
https://www.apache.org/licenses/LICENSE-2.0> or the MIT
license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
at your option. All files in the project carrying such
notice may not be copied, modified, or distributed except
according to those terms.

This crate includes data derived from the data files supplied
with the WHATWG Encoding Standard, which, when incorporated into
source code, are licensed under the BSD 3-Clause License
<LICENSE-WHATWG>.

Test code within encoding_rs is dedicated to the Public Domain when so
designated (see the individual files for PD/CC0-dedicated sections).
```

#### `encoding_rs` 0.8.44: `LICENSE-WHATWG`

```text
Copyright © WHATWG (Apple, Google, Mozilla, Microsoft).

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

#### `euclid` 0.20.14: `COPYRIGHT`

```text
Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
<LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
option. All files in the project carrying such notice may not be
copied, modified, or distributed except according to those terms.
```

#### `foldhash` 0.1.5: `LICENSE`

```text
Copyright (c) 2024 Orson Peters

This software is provided 'as-is', without any express or implied warranty. In
no event will the authors be held liable for any damages arising from the use of
this software.

Permission is granted to anyone to use this software for any purpose, including
commercial applications, and to alter it and redistribute it freely, subject to
the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim
    that you wrote the original software. If you use this software in a product,
    an acknowledgment in the product documentation would be appreciated but is
    not required.

2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### `glow` 0.16.0: `LICENSE-ZLIB`

```text
This software is provided 'as-is', without any express or implied
warranty.  In no event will the authors be held liable for any damages
arising from the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not
   claim that you wrote the original software. If you use this software
   in a product, an acknowledgment in the product documentation would be
   appreciated but is not required.
2. Altered source versions must be plainly marked as such, and must not be
   misrepresented as being the original software.
3. This notice may not be removed or altered from any source distribution.
```

#### `libloading` 0.8.9: `LICENSE`

```text
Copyright © 2015, Simonas Kazlauskas

Permission to use, copy, modify, and/or distribute this software for any purpose with or without
fee is hereby granted, provided that the above copyright notice and this permission notice appear
in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS
SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE
AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT,
NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
THIS SOFTWARE.
```

#### `linux-raw-sys` 0.12.1: `COPYRIGHT`

```text
Short version for non-lawyers:

`linux-raw-sys` is triple-licensed under Apache 2.0 with the LLVM Exception,
Apache 2.0, and MIT terms.


Longer version:

Copyrights in the `linux-raw-sys` project are retained by their contributors.
No copyright assignment is required to contribute to the `linux-raw-sys`
project.

Some files include code derived from Rust's `libstd`; see the comments in
the code for details.

Except as otherwise noted (below and/or in individual files), `linux-raw-sys`
is licensed under:

 - the Apache License, Version 2.0, with the LLVM Exception
   <LICENSE-Apache-2.0_WITH_LLVM-exception> or
   <http://llvm.org/foundation/relicensing/LICENSE.txt>
 - the Apache License, Version 2.0
   <LICENSE-APACHE> or
   <http://www.apache.org/licenses/LICENSE-2.0>,
 - or the MIT license
   <LICENSE-MIT> or
   <http://opensource.org/licenses/MIT>,

at your option.
```

#### `linux-raw-sys` 0.4.15: `COPYRIGHT`

```text
Short version for non-lawyers:

`linux-raw-sys` is triple-licensed under Apache 2.0 with the LLVM Exception,
Apache 2.0, and MIT terms.


Longer version:

Copyrights in the `linux-raw-sys` project are retained by their contributors.
No copyright assignment is required to contribute to the `linux-raw-sys`
project.

Some files include code derived from Rust's `libstd`; see the comments in
the code for details.

Except as otherwise noted (below and/or in individual files), `linux-raw-sys`
is licensed under:

 - the Apache License, Version 2.0, with the LLVM Exception
   <LICENSE-Apache-2.0_WITH_LLVM-exception> or
   <http://llvm.org/foundation/relicensing/LICENSE.txt>
 - the Apache License, Version 2.0
   <LICENSE-APACHE> or
   <http://www.apache.org/licenses/LICENSE-2.0>,
 - or the MIT license
   <LICENSE-MIT> or
   <http://opensource.org/licenses/MIT>,

at your option.
```

#### `memchr` 2.8.3: `COPYING`

```text
This project is dual-licensed under the Unlicense and MIT licenses.

You may use this code under the terms of either license.
```

#### `memchr` 2.8.3: `UNLICENSE`

```text
This is free and unencumbered software released into the public domain.

Anyone is free to copy, modify, publish, use, compile, sell, or
distribute this software, either in source code form or as a compiled
binary, for any purpose, commercial or non-commercial, and by any
means.

In jurisdictions that recognize copyright laws, the author or authors
of this software dedicate any and all copyright interest in the
software to the public domain. We make this dedication for the benefit
of the public at large and to the detriment of our heirs and
successors. We intend this dedication to be an overt act of
relinquishment in perpetuity of all present and future rights to this
software under copyright law.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

For more information, please refer to <http://unlicense.org/>
```

#### `miniz_oxide` 0.8.9: `LICENSE-ZLIB.md`

```text
Copyright 2013-2014 RAD Game Tools and Valve Software
Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC
Copyright (c) 2020 Frommi
Copyright (c) 2017-2024 oyvindln

This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### `miniz_oxide` 0.9.1: `LICENSE-ZLIB.md`

```text
Copyright 2013-2014 RAD Game Tools and Valve Software
Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC
Copyright (c) 2020 Frommi
Copyright (c) 2017-2024 oyvindln

This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### `moxcms` 0.8.1: `LICENSE.md`

```text
Copyright (c) Radzivon Bartoshyk. All rights reserved.

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

1.  Redistributions of source code must retain the above copyright notice, this
    list of conditions and the following disclaimer.

2.  Redistributions in binary form must reproduce the above copyright notice,
    this list of conditions and the following disclaimer in the documentation
    and/or other materials provided with the distribution.

3.  Neither the name of the copyright holder nor the names of its
    contributors may be used to endorse or promote products derived from
    this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

#### `multiversion_no_op` 1.0.0: `COPYRIGHT`

```text
Copyright Mozilla Foundation

Licensed under the Apache License (Version 2.0), or the MIT license,
(the "Licenses") at your option. You may not use this file except in
compliance with one of the Licenses. You may obtain copies of the
Licenses at:

   https://www.apache.org/licenses/LICENSE-2.0
   https://opensource.org/licenses/MIT

Unless required by applicable law or agreed to in writing, software
distributed under the Licenses is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the Licenses for the specific language governing permissions and
limitations under the Licenses.
```

#### `openssl` 0.10.82: `LICENSE`

```text
Copyright 2011-2017 Google Inc.
          2013 Jack Lloyd
          2013-2014 Steven Fackler

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

#### `parking` 2.2.1: `LICENSE-THIRD-PARTY`

```text
===============================================================================

Copyright 2014-2020 The Rust Project Developers

Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
<LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
option. All files in the project carrying such notice may not be
copied, modified, or distributed except according to those terms.
```

#### `pxfm` 0.1.30: `LICENSE.md`

```text
Copyright (c) Radzivon Bartoshyk. All rights reserved.

Redistribution and use in source and binary forms, with or without modification,
are permitted provided that the following conditions are met:

1.  Redistributions of source code must retain the above copyright notice, this
    list of conditions and the following disclaimer.

2.  Redistributions in binary form must reproduce the above copyright notice,
    this list of conditions and the following disclaimer in the documentation
    and/or other materials provided with the distribution.

3.  Neither the name of the copyright holder nor the names of its
    contributors may be used to endorse or promote products derived from
    this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

#### `rand` 0.10.3: `COPYRIGHT`

```text
Copyrights in the Rand project are retained by their contributors. No
copyright assignment is required to contribute to the Rand project.

For full authorship information, see the version control history.

Except as otherwise noted (below and/or in individual files), Rand is
licensed under the Apache License, Version 2.0 <LICENSE-APACHE> or
<http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
<LICENSE-MIT> or <http://opensource.org/licenses/MIT>, at your option.

The Rand project includes code from the Rust project
published under these same licenses.
```

#### `rand` 0.9.5: `COPYRIGHT`

```text
Copyrights in the Rand project are retained by their contributors. No
copyright assignment is required to contribute to the Rand project.

For full authorship information, see the version control history.

Except as otherwise noted (below and/or in individual files), Rand is
licensed under the Apache License, Version 2.0 <LICENSE-APACHE> or
<http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
<LICENSE-MIT> or <http://opensource.org/licenses/MIT>, at your option.

The Rand project includes code from the Rust project
published under these same licenses.
```

#### `rand_chacha` 0.9.0: `COPYRIGHT`

```text
Copyrights in the Rand project are retained by their contributors. No
copyright assignment is required to contribute to the Rand project.

For full authorship information, see the version control history.

Except as otherwise noted (below and/or in individual files), Rand is
licensed under the Apache License, Version 2.0 <LICENSE-APACHE> or
<http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
<LICENSE-MIT> or <http://opensource.org/licenses/MIT>, at your option.

The Rand project includes code from the Rust project
published under these same licenses.
```

#### `rand_core` 0.10.1: `COPYRIGHT`

```text
Copyrights in the Rand project are retained by their contributors. No
copyright assignment is required to contribute to the Rand project.

For full authorship information, see the version control history.

Except as otherwise noted (below and/or in individual files), Rand is
licensed under the Apache License, Version 2.0 <LICENSE-APACHE> or
<http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
<LICENSE-MIT> or <http://opensource.org/licenses/MIT>, at your option.
```

#### `rand_core` 0.9.5: `COPYRIGHT`

```text
Copyrights in the Rand project are retained by their contributors. No
copyright assignment is required to contribute to the Rand project.

For full authorship information, see the version control history.

Except as otherwise noted (below and/or in individual files), Rand is
licensed under the Apache License, Version 2.0 <LICENSE-APACHE> or
<http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
<LICENSE-MIT> or <http://opensource.org/licenses/MIT>, at your option.

The Rand project includes code from the Rust project
published under these same licenses.
```

#### `raw-window-handle` 0.6.2: `LICENSE-ZLIB.md`

```text
Copyright (c) 2020 Osspial

This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### `rustix` 0.38.44: `COPYRIGHT`

```text
Short version for non-lawyers:

`rustix` is triple-licensed under Apache 2.0 with the LLVM Exception,
Apache 2.0, and MIT terms.


Longer version:

Copyrights in the `rustix` project are retained by their contributors.
No copyright assignment is required to contribute to the `rustix`
project.

Some files include code derived from Rust's `libstd`; see the comments in
the code for details.

Except as otherwise noted (below and/or in individual files), `rustix`
is licensed under:

 - the Apache License, Version 2.0, with the LLVM Exception
   <LICENSE-Apache-2.0_WITH_LLVM-exception> or
   <http://llvm.org/foundation/relicensing/LICENSE.txt>
 - the Apache License, Version 2.0
   <LICENSE-APACHE> or
   <http://www.apache.org/licenses/LICENSE-2.0>,
 - or the MIT license
   <LICENSE-MIT> or
   <http://opensource.org/licenses/MIT>,

at your option.
```

#### `rustix` 1.1.6: `COPYRIGHT`

```text
Short version for non-lawyers:

`rustix` is triple-licensed under Apache 2.0 with the LLVM Exception,
Apache 2.0, and MIT terms.


Longer version:

Copyrights in the `rustix` project are retained by their contributors.
No copyright assignment is required to contribute to the `rustix`
project.

Some files include code derived from Rust's `libstd`; see the comments in
the code for details.

Except as otherwise noted (below and/or in individual files), `rustix`
is licensed under:

 - the Apache License, Version 2.0, with the LLVM Exception
   <LICENSE-Apache-2.0_WITH_LLVM-exception> or
   <http://llvm.org/foundation/relicensing/LICENSE.txt>
 - the Apache License, Version 2.0
   <LICENSE-APACHE> or
   <http://www.apache.org/licenses/LICENSE-2.0>,
 - or the MIT license
   <LICENSE-MIT> or
   <http://opensource.org/licenses/MIT>,

at your option.
```

#### `tinyvec` 1.13.3: `LICENSE-ZLIB.md`

```text
Copyright (c) 2019 Daniel "Lokathor" Gee.

This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### `typenum` 1.20.1: `LICENSE`

```text
MIT OR Apache-2.0
```

#### `unicode-bidi` 0.3.18: `COPYRIGHT`

```text
This project is copyright 2015, The Servo Project Developers (given in the
file AUTHORS).

Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
http://opensource.org/licenses/MIT>, at your option. All files in the project
carrying such notice may not be copied, modified, or distributed except
according to those terms.
```

#### `unicode-normalization` 0.1.26: `COPYRIGHT`

```text
Licensed under the Apache License, Version 2.0
<LICENSE-APACHE or
http://www.apache.org/licenses/LICENSE-2.0> or the MIT
license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
at your option. All files in the project carrying such
notice may not be copied, modified, or distributed except
according to those terms.
```

#### `unicode-properties` 0.1.5: `COPYRIGHT`

```text
Licensed under the Apache License, Version 2.0
<LICENSE-APACHE or
http://www.apache.org/licenses/LICENSE-2.0> or the MIT
license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
at your option. All files in the project carrying such
notice may not be copied, modified, or distributed except
according to those terms.
```

#### `unicode-segmentation` 1.13.3: `COPYRIGHT`

```text
Licensed under the Apache License, Version 2.0
<LICENSE-APACHE or
http://www.apache.org/licenses/LICENSE-2.0> or the MIT
license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
at your option. All files in the project carrying such
notice may not be copied, modified, or distributed except
according to those terms.
```

#### `utf8_iter` 1.1.0: `COPYRIGHT`

```text
Copyright Mozilla Foundation

Licensed under the Apache License (Version 2.0), or the MIT license,
(the "Licenses") at your option. You may not use this file except in
compliance with one of the Licenses. You may obtain copies of the
Licenses at:

   https://www.apache.org/licenses/LICENSE-2.0
   https://opensource.org/licenses/MIT

Unless required by applicable law or agreed to in writing, software
distributed under the Licenses is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the Licenses for the specific language governing permissions and
limitations under the Licenses.

--

Test code is dedicated to the Public Domain when so designated (see
the individual files for PD/CC0-dedicated sections).

--

The implementation for Utf8CharIndices was adapted from the
CharIndices implementation of the Rust standard library at revision
ab32548539ec38a939c1b58599249f3b54130026
(https://github.com/rust-lang/rust/blob/ab32548539ec38a939c1b58599249f3b54130026/library/core/src/str/iter.rs).

Excerpt from https://github.com/rust-lang/rust/blob/ab32548539ec38a939c1b58599249f3b54130026/COPYRIGHT ,
which refers to
https://github.com/rust-lang/rust/blob/ab32548539ec38a939c1b58599249f3b54130026/LICENSE-APACHE
and
https://github.com/rust-lang/rust/blob/ab32548539ec38a939c1b58599249f3b54130026/LICENSE-MIT
:

For full authorship information, see the version control history or
https://thanks.rust-lang.org

Except as otherwise noted (below and/or in individual files), Rust is
licensed under the Apache License, Version 2.0 <LICENSE-APACHE> or
<http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
<LICENSE-MIT> or <http://opensource.org/licenses/MIT>, at your option.
```

#### `utf_types` 1.0.0: `COPYRIGHT`

```text
Copyright Mozilla Foundation

Licensed under the Apache License (Version 2.0), or the MIT license,
(the "Licenses") at your option. You may not use this file except in
compliance with one of the Licenses. You may obtain copies of the
Licenses at:

   https://www.apache.org/licenses/LICENSE-2.0
   https://opensource.org/licenses/MIT

Unless required by applicable law or agreed to in writing, software
distributed under the Licenses is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the Licenses for the specific language governing permissions and
limitations under the Licenses.

--

Test code is dedicated to the Public Domain when so designated (see
the individual files for PD/CC0-dedicated sections).
```

#### `xkeysym` 0.2.1: `LICENSE-ZLIB`

```text
Copyright (c) 2022-2023 John Nunley

This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

#### `zerocopy` 0.8.63: `LICENSE-BSD`

```text
Copyright 2019 The Fuchsia Authors.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

   * Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above
copyright notice, this list of conditions and the following disclaimer
in the documentation and/or other materials provided with the
distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

#### `zlib-rs` 0.6.8: `LICENSE`

```text
(C) 2024 Trifecta Tech Foundation 

This software is provided 'as-is', without any express or implied
warranty. In no event will the authors be held liable for any damages
arising from the use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not
   claim that you wrote the original software. If you use this software
   in a product, an acknowledgment in the product documentation would be
   appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be
   misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

<!-- END generated crate notices -->

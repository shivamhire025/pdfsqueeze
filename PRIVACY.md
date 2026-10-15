# pdfsqueeze: privacy

> **DRAFT for review by a lawyer before the first sale or email signup.** Placeholders in
> [SQUARE BRACKETS] must be filled in. This is not legal advice.

Last updated: [DATE]

Publisher: **[PUBLISHER LEGAL NAME]**, contact [CONTACT EMAIL].

## The short version

- **Your PDFs and images stay on your computer.** The contents of the files you open are
  processed on your machine and are never sent to the publisher or to anyone else.
- **The app does not phone home.** There is no analytics, no telemetry, no crash reporting, no
  account and no automatic update check in the app. The program contains no code that makes
  outgoing network connections; the only network socket it opens is a private one on your own
  computer (127.0.0.1) so its window can talk to the program.
- **The website sets no cookies** and uses no analytics.
- **We only receive personal data if you buy a licence or join the mailing list,** and then only
  your email address and what a purchase needs.

## The app

The app runs the tools Ghostscript, qpdf and Tesseract locally on files you choose. Results are
written to your Downloads folder as new files. A temporary working folder in your system's temporary
directory holds copies and previews while the window is open and is deleted when you close it.

The window is shown by your operating system's own web view (WebKit on macOS, WebView2 on Windows).
That component belongs to your operating system and is governed by its vendor's terms and privacy
policy, not by this one. The app only loads its own page from your computer.

[WHEN LICENCE ACTIVATION IS ADDED: describe exactly what is sent (the licence key, once, to
[LICENCE SERVICE]) and that it is the only network request the app makes. Update this page before
that version is released.]

## The website ([OFFICIAL SITE])

The site is a static page hosted on GitHub Pages. It sets no cookies, loads no third-party scripts,
fonts or trackers, and embeds no checkout. GitHub, as the host, may log visitors' IP addresses under
its own privacy statement.

## Buying a licence

Payments are handled by **[Lemon Squeezy]**, which is the seller of record. It collects your name,
email, billing country and payment details under its own privacy policy. We receive [your email
address, order number, country and what you purchased: VERIFY what the platform passes on] and use
it to deliver your licence key, send your receipt, give support, and handle refunds and tax records.
We keep it for [PERIOD, e.g. as long as you hold a licence plus the tax record period].

## The mailing list

If you choose to join, we store your email address with [NEWSLETTER PROVIDER] and use it only to
send you pdfsqueeze news. Joining is separate from buying and from downloading; you can use the
software without it. Every message has an unsubscribe link and you can ask us to delete your address
at any time. We do not sell or share the list. If the project is transferred to a new owner, the list
may transfer with it, and you will be told and can unsubscribe. [LAWYER: confirm wording for CASL
and GDPR.]

## Your choices and rights

You can ask us at [CONTACT EMAIL] to see, correct or delete the personal data we hold about you.
Depending on where you live (for example Canada, the EU or the UK) you may have further rights,
including to complain to your data protection authority.

## Children

pdfsqueeze is not directed at children and we do not knowingly collect their data.

## Changes

If this policy changes we will update the date above, and for material changes tell mailing-list
subscribers and licence holders.

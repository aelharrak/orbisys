# OrbiSys

**صيانة وإدارة أنظمة Linux في تطبيق واحد.** يجمع OrbiSys بين واجهة رسومية وسطر أوامر (CLI) مبنيين على نواة Rust مشتركة.

[![الإصدار](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml) [![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/) [![رخصة MIT](https://img.shields.io/badge/license-MIT-blue)](Cargo.toml)

<p align="center"><img src="packaging/orbisys.svg" alt="شعار OrbiSys" width="112"></p>

**اللغات:** [English](README.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Español](README.es.md) · [Português](README.pt.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [中文](README.zh.md) · العربية

## الميزات

يجمع OrbiSys أدوات فحص النظام وصيانته:

- **النظام والمراقبة:** حالة الجهاز، والعمليات، والخدمات، والسجلات، وعناصر بدء التشغيل، والمهام المجدولة.
- **البرامج:** جرد الحزم الأصلية وSnap وFlatpak والتطبيقات المثبتة يدوياً؛ والتحديثات والمستودعات وبيئات التشغيل.
- **التخزين والنسخ الاحتياطي:** الأقراص، ونقاط الربط، والتنظيف، والأرشيفات، وBtrfs، واللقطات، والنسخ الاحتياطية.
- **الشبكة والأجهزة:** الشبكة المحلية، وحركة المرور، وWi-Fi، وVPN، وBluetooth، وUSB، والطاقة، والمستشعرات، والبرامج الثابتة.
- **الأمان:** التدقيق، والجدار الناري، وSecure Boot، وAppArmor، والشهادات، ومفاتيح SSH/GPG، وفحوصات السلامة.
- **التصدير:** جرد بصيغ TXT أو CSV أو JSON؛ وتقارير النظام بصيغ TXT أو CSV أو JSON أو PDF.

تعتمد بعض الميزات على أدوات النظام المتوفرة في التوزيعة. ويمكن ضبط مظهر الواجهة ولغتها.

## التثبيت

تتوفر الحزم المبنية مسبقاً في [`dist/`](dist/) للبُنى الموضحة في أسماء الملفات. من جذر المستودع، اختر الحزمة المناسبة لتوزيعتك.

| التوزيعة | أمر التثبيت |
| --- | --- |
| Fedora وRHEL والتوزيعات المتوافقة | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian وUbuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux والتوزيعات المشتقة | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

حزمة Alpine المرفقة ملف محلي غير موقّع من مستودع مُعدّ، لذا يلزم الخيار `--allow-untrusted`. تثبّت مدراء الحزم الأخرى الملف المحلي وتحلّ التبعيات المعلنة. يلزم Polkit للعمليات ذات الامتيازات؛ توصي به حزمة Debian وتعلنه الصيغ الأخرى كتَبَعية.

بعد التثبيت، شغّل `orbisys` في الطرفية أو افتح **OrbiSys** من قائمة التطبيقات.

## إزالة التثبيت

استخدم مدير الحزم في توزيعتك لإزالة OrbiSys:

| التوزيعة | الأمر |
| --- | --- |
| Fedora وRHEL | `sudo dnf remove orbisys` |
| openSUSE | `sudo zypper remove orbisys` |
| Debian وUbuntu | `sudo apt remove orbisys` |
| Arch Linux | `sudo pacman -R orbisys` |
| Alpine Linux | `sudo apk del orbisys` |

إذا ثبّت OrbiSys من مستودع PPA، فأزل المستودع بشكل منفصل باستخدام `sudo add-apt-repository --remove ppa:amine-elharrak/ppa`. إذا أنشأت التطبيق من المصدر، فأغلقه ثم احذف مجلد المستودع المستنسخ.

### البناء من المصدر

المتطلبات: Rust وCargo بإصدار 1.80 أو أحدث. من جذر المشروع نفّذ:

```sh
cargo build --release
./target/release/orbisys
```

من دون وسيطات، يبدأ التطبيق بواجهته الرسومية. لاستخدام CLI شغّل `./target/release/orbisys help`. أثناء التطوير يمكن استخدام `cargo run -- help` مثلاً.

## البدء السريع

```sh
orbisys help
orbisys version
orbisys inventory
orbisys inventory --csv software.csv
orbisys updates
orbisys clean                 # فحص دون حذف
orbisys report ./reports
orbisys panel                 # عرض الأقسام
orbisys panel monitor
```

## أوامر CLI

| الأمر | الوصف |
| --- | --- |
| `help` أو `--help` أو `-h` | عرض المساعدة. |
| `version` أو `--version` | عرض الإصدار. |
| `inventory [--txt FILE] [--csv FILE] [--json FILE]` | عرض جرد البرامج وكتابة الصيغ المطلوبة. |
| `updates` | التحقق من التحديثات عبر مدير الحزم المكتشف. |
| `clean` | عرض عناصر التنظيف وأحجامها دون حذفها. |
| `clean --apply` | تنظيف العناصر الشخصية المدعومة؛ وتتجاوز CLI عناصر النظام التي تتطلب root. |
| `report [DIRECTORY]` | كتابة تقارير TXT وCSV وJSON وPDF في المجلد المحدد أو مجلد Documents الافتراضي. |
| `panel` / `panel NAME` | عرض الأقسام / عرض قسم بوضع القراءة فقط. |

يعرض `orbisys panel` الأسماء المقبولة، مثل `monitor` و`health` و`mounts` و`lan` و`certs`.

## الأمان والأذونات

لا تغيّر شاشات الاستعراض وأوامر التشخيص النظام. تُنفّذ العمليات التي تتطلب امتيازات عبر Polkit (`pkexec`) وتتطلب تفويضاً. في CLI، يعالج `clean --apply` العناصر الشخصية فقط.

يشغّل OrbiSys البرامج بوسائط منفصلة ويتحقق من أسماء الحزم والوحدات والمسارات قبل استخدامها. وقد تختلف أدوات النظام المتوفرة حسب التوزيعة.

## بناء الحزم

تنشئ البرامج النصية في [`packaging/`](packaging/) حزم DEB وRPM وArch وAlpine انطلاقاً من ملف الإصدار التنفيذي:

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
# محاولة بناء كل الصيغ
./packaging/build-all.sh
```

الأدوات المطلوبة: `dpkg-deb` لـDEB، و`rpmbuild` لـRPM، و`tar` و`zstd` لـArch، و`apk-tools` 3 أو Docker لـAlpine. تُحفظ الحزم في `dist/`. في بعض أنظمة Debian/Ubuntu يمكن لبرنامج RPM النصي جلب أدواته محلياً.

## اللغات

تتوفر الواجهة بالفرنسية والإنجليزية والألمانية والإسبانية والبرتغالية والإيطالية والهولندية واليابانية والكورية والصينية والعربية. الترجمات الفرنسية والإنجليزية والإسبانية مكتملة؛ وقد تكون الترجمات الأخرى جزئية، وتظهر النصوص غير المترجمة بالإنجليزية.

## بنية المشروع

- `src/main.rs`: نقطة الدخول والاختيار بين GUI وCLI.
- `src/lib.rs`: النواة المشتركة ووحدات النظام.
- `src/cli.rs`: أوامر CLI.
- `src/gui.rs` و`src/gui/`: الواجهة الرسومية.
- `src/p_*.rs` و`src/panels.rs`: الأقسام المتخصصة.
- `src/lang/`: ملفات ترجمة التطبيق.
- `packaging/`: البرامج النصية وبيانات حزم Linux.
- `assets/`: موارد رسومية؛ `dist/`: الحزم المرفقة المبنية مسبقاً.

## الترخيص

يعلن المشروع ترخيص MIT في [`Cargo.toml`](Cargo.toml). ولأيقونات Lucide المرفقة ترخيص منفصل موضح في [`assets/icons/LICENSE`](assets/icons/LICENSE).

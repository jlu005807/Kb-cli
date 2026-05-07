# KB-CLI 瀹夎涓庢墦鍖呮寚鍗楋紙Windows / Linux锛?
鏈枃妗ｈ鏄庡浣曟妸 `kb-cli` 鎵撳寘涓哄彲鍒嗗彂瀹夎鍖咃紝骞跺湪鐢ㄦ埛鏈哄櫒涓婁竴閿畨瑁咃紝鍚屾椂鑷姩閰嶇疆鐜鍙橀噺锛圥ATH锛夈€?
## 1. 浜х墿涓庣洰褰曠害瀹?
- Windows 鎵撳寘鑴氭湰锛歚scripts/package.ps1`
- Linux 鎵撳寘鑴氭湰锛歚scripts/package.sh`
- Windows 瀹夎妯℃澘锛歚packaging/windows/*`
- Linux 瀹夎妯℃澘锛歚packaging/linux/*`
- 榛樿杈撳嚭鐩綍锛歚dist/`

## 2. Windows 鎵撳寘涓庡畨瑁?
### 2.1 鏈湴鎵撳寘

鍦ㄤ粨搴撴牴鐩綍鎵ц锛?
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1
```

鍙€夊弬鏁帮細

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1 `
  -Target x86_64-pc-windows-msvc `
  -Profile release `
  -DistDir dist
```

### 2.2 鎵撳寘浜х墿

绀轰緥锛?
```text
dist/
鈹斺攢 kb-cli-0.1.4-windows-x86_64.zip
   鈹斺攢 kb-cli-0.1.4-windows-x86_64/
      鈹溾攢 kb.exe
      鈹溾攢 install.cmd
      鈹溾攢 install.ps1
      鈹溾攢 uninstall.cmd
      鈹斺攢 uninstall.ps1
```

### 2.3 涓€閿畨瑁?
- 鍙屽嚮 `install.cmd`锛堟帹鑽愶紝宸插唴缃?`ExecutionPolicy Bypass`锛?- 鎴栧懡浠よ鎵ц锛?
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

瀹夎琛屼负锛?
- 灏嗗彲鎵ц鏂囦欢瀹夎鍒扮敤鎴风洰褰曪細`%LOCALAPPDATA%\kb-cli\bin\kb.exe`
- 鑷姩鍐欏叆鐢ㄦ埛绾?`PATH`
- 褰撳墠浼氳瘽鍐呭皾璇曞嵆鏃舵敞鍏?PATH锛堟柊缁堢涓€瀹氱敓鏁堬級

### 2.4 鍗歌浇

- 鍙屽嚮 `uninstall.cmd`
- 鎴栨墽琛岋細

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\uninstall.ps1
```

鍗歌浇琛屼负锛?
- 鍒犻櫎宸插畨瑁呬簩杩涘埗
- 娓呯悊鐢ㄦ埛绾?PATH 涓殑瀹夎鐩綍
- 瀹夎鐩綍涓虹┖鏃惰嚜鍔ㄥ垹闄?
## 3. Linux 鎵撳寘涓庡畨瑁?
### 3.1 鏈湴鎵撳寘

鍦ㄤ粨搴撴牴鐩綍鎵ц锛?
```bash
bash ./scripts/package.sh
```

鍙€夊弬鏁帮細

```bash
bash ./scripts/package.sh \
  --target x86_64-unknown-linux-gnu \
  --profile release \
  --dist-dir dist
```

### 3.1.1 鍦?Git Bash锛圵indows锛変腑鎵?Linux 鍖?
鍙互锛屼絾寤鸿鏄惧紡鎸囧畾 Linux target锛屽苟鍏堝畨瑁呭搴?Rust target锛?
```bash
rustup target add x86_64-unknown-linux-gnu
bash ./scripts/package.sh --target x86_64-unknown-linux-gnu
```

璇存槑锛?
- `scripts/package.sh` 鍦?Git Bash/MSYS/Cygwin 鐜涓嬶紝鑻ユ湭浼?`--target` 浼氱洿鎺ユ姤閿欏苟缁欏嚭鎻愮ず銆?- 璇ヨ剼鏈彧鐢ㄤ簬 Linux 浜х墿鎵撳寘锛沇indows 浜х墿璇蜂娇鐢?`scripts/package.ps1`銆?
### 3.2 鎵撳寘浜х墿

绀轰緥锛?
```text
dist/
鈹斺攢 kb-cli-0.1.4-linux-x86_64.tar.gz
   鈹斺攢 kb-cli-0.1.4-linux-x86_64/
      鈹溾攢 kb
      鈹溾攢 install.sh
      鈹斺攢 uninstall.sh
```

### 3.3 涓€閿畨瑁?
```bash
bash ./install.sh
```

瀹夎琛屼负锛?
- 灏嗕簩杩涘埗瀹夎鍒?`~/.local/bin/kb`锛堥粯璁わ級
- 鑻ュ綋鍓?`PATH` 鏈寘鍚畨瑁呯洰褰曪紝鑷姩杩藉姞鍒帮細
  - `~/.bashrc`
  - `~/.zshrc`
  - `~/.profile`
- 閬垮厤閲嶅鍐欏叆鍚屼竴琛?PATH 閰嶇疆

### 3.4 鍗歌浇

```bash
bash ./uninstall.sh
```

鍗歌浇琛屼负锛?
- 鍒犻櫎瀹夎鐨?`kb` 浜岃繘鍒?- 浠?shell 閰嶇疆鏂囦欢涓竻鐞嗗畨瑁呮椂杩藉姞鐨?PATH 琛?- 瀹夎鐩綍涓虹┖鏃惰嚜鍔ㄥ垹闄?
## 4. 鑷畾涔夊畨瑁呯洰褰曪紙楂樼骇锛?
### 4.1 Linux

閫氳繃鐜鍙橀噺瑕嗙洊瀹夎鐩綍/鍛戒护鍚嶏細

```bash
KB_INSTALL_DIR="$HOME/tools/bin" KB_BIN_NAME="kb" bash ./install.sh
```

### 4.2 Windows

閫氳繃鍙傛暟瑕嗙洊瀹夎鐩綍/鍛戒护鍚嶏細

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 `
  -InstallDir "$env:LOCALAPPDATA\kb-cli\bin" `
  -BinaryName "kb.exe"
```

## 5. 楠岃瘉瀹夎

```bash
kb --help
kb --version
```

鑻ュ懡浠ゆ湭鐢熸晥锛岃閲嶅紑缁堢鍚庨噸璇曘€?
## 6. GitHub 鑷姩鎵撳寘骞跺彂甯?Release

浠撳簱宸叉彁渚?workflow锛歚.github/workflows/release.yml`銆?
瑙﹀彂鏂瑰紡锛?
- 鎺ㄩ€?tag锛坄v*`锛夋椂鑷姩瑙﹀彂锛屼緥濡?`v0.1.4`
- 涔熷彲鍦?GitHub Actions 椤甸潰鎵嬪姩瑙﹀彂锛坄workflow_dispatch`锛?
娴佺▼鍐呭锛?
- Ubuntu runner 鏋勫缓骞舵墦鍖?Linux 浜х墿锛坄.tar.gz`锛?- Windows runner 鏋勫缓骞舵墦鍖?Windows 浜х墿锛坄.zip`锛?- 鑷姩鍒涘缓/鏇存柊瀵瑰簲 GitHub Release锛屽苟涓婁紶涓婅堪浜х墿闄勪欢

### 6.1 鎺ㄨ崘鍙戝竷姝ラ

```bash
git tag v0.1.4
git push origin v0.1.4
```

鎺ㄩ€佸畬鎴愬悗锛岀瓑寰?Actions 鎵ц鎴愬姛锛屽嵆鍙湪 Releases 椤甸潰鐪嬪埌瀵瑰簲鐗堟湰涓庨檮浠躲€?
## 7. 甯歌闂锛圵indows锛?
### 7.1 杩愯 `.\scripts\package.ps1` 鎻愮ず鈥滀笉鑳藉 Null 鍊艰〃杈惧紡璋冪敤鏂规硶鈥?
宸插湪褰撳墠鑴氭湰瀹炵幇涓鍔犱互涓嬮槻鎶わ細

- 鏋舵瀯鎺㈡祴鏀寔澶氶噸鍥為€€锛坄RuntimeInformation` -> `PROCESSOR_ARCHITECTURE` -> 榛樿鍊硷級
- 缂哄け渚濊禆锛坄cargo` / `rustup`锛変細缁欏嚭鍙閿欒锛岃€屼笉鏄┖寮曠敤寮傚父
- 缂哄け瀹夎妯℃澘鏂囦欢浼氬湪鎵撳寘鍓嶆樉寮忔姤閿?
寤鸿鎺掓煡姝ラ锛?
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\package.ps1
```

鑻ヤ粛澶辫触锛岃鍏堟墽琛岋細

```powershell
cargo --version
rustup --version
```


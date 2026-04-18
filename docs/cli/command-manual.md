# KB-CLI 鍛戒护鎵嬪唽

鏈枃妗ｄ负 KB-CLI 鍛戒护浣撶郴鐨?*鏉冨▉鍙傝€?*锛岀敤浜庤鏄庡懡浠よ娉曘€佸弬鏁般€侀€夐」銆佽涔夈€侀敊璇涓轰笌杈圭晫鏉′欢銆?
> **鏉冨▉澹版槑锛?* 褰?`command-examples.md` 鎴?`error-catalog.md` 涓庢湰鎵嬪唽涓嶄竴鑷存椂锛屼互鏈墜鍐屼负鍑嗐€?>
> **鍏宠仈鏂囨。锛?*
> - `docs/cli/kb-md-spec.md`
> - `docs/cli/state-diff-contract.md`
> - `docs/cli/kbignore-spec.md`
>
> **寮€鍙戣鏄庯細** CLI 瑙ｆ瀽寤鸿浣跨敤 `clap` derive 瀹忓疄鐜般€?
> **瀹炵幇鐘舵€佽ˉ鍏咃紙2026-04-18锛夛細**
> - `rm --force` 涓?`update --yes` 褰撳墠涓轰繚鐣欏弬鏁颁綅锛屼笉鏀瑰彉鎵ц閫昏緫銆?> - `check/update` 澹版槑杈撳叆鏉ユ簮涓?`kb.md + ./.kb/raw/`锛岀櫥璁版潵婧愪负 `./.kb/regs.json`銆?> - `list` 褰撳墠璇诲彇 `./.kb/regs.json`锛堣€屼笉鏄洿鎺ユ壂鎻?`kb.md`锛夈€?> - `list --long` 鐨?`mtime` 褰撳墠杈撳嚭 Unix 绉掑瓧绗︿覆銆?> - `init` 鎴愬姛鍚庝細灏濊瘯鐧昏褰撳墠鐭ヨ瘑搴撳埌鍏ㄥ眬娉ㄥ唽琛紙鐧昏澶辫触涓嶅奖鍝?init 鎴愬姛锛夈€?> - 鏂板 `kb env --list`锛氱敤浜庡垪鍑哄凡鐧昏涓斾粛瀛樺湪鐨勭煡璇嗗簱鏍圭洰褰曘€?> - 鏍瑰懡浠ゅ府鍔╁悕涓?`kb`锛孶sage 涓?`kb [OPTIONS] <COMMAND>`锛屽苟鎻愪緵鍒嗙粍姒傝娈佃惤銆?> - text 妯″紡浣跨敤鍛戒护涓撳睘澶氳杈撳嚭锛沯son 妯″紡淇濇寔绋冲畾缁撴瀯骞剁Щ闄ゅ唴閮?`_raw_text` 瀛楁銆?> - 琛ュ厖 `kb completion` 鍛戒护濂戠害涓?`kb -h` 杈撳嚭闄勫綍锛屾枃妗ｄ笌褰撳墠瀹炵幇淇濇寔涓€鑷淬€?> - `kb update` 浼氬皢鏂板 raw 鏂囦欢鎸?`kb.md` 瑙勮寖鍥炲～鍒?`# raw` 鍒嗙被锛屼笖鍘婚噸銆?
## 鍏ㄥ眬閫夐」

| 閫夐」 | 绫诲瀷 | 榛樿鍊?| 璇存槑 |
|---|---|---|---|
| `--output <text\|json>` | 鏋氫妇 | `text` | 璁剧疆杈撳嚭鏍煎紡 |
| `--quiet` | 鏍囧織 | 鍏抽棴 | 鎶戝埗鎵€鏈夐潪蹇呰杈撳嚭 |
| `--verbose` | 璁℃暟 | `0` | 鎻愰珮鏃ュ織璇︾粏绋嬪害锛屽彲鍙犲姞 `-v` `-vv` |
| `--config <PATH>` | 瀛楃涓?| 骞冲彴榛樿 | 瑕嗙洊榛樿閰嶇疆鏂囦欢璺緞 |
| `--no-color` | 鏍囧織 | 鍏抽棴 | 绂佺敤杈撳嚭涓殑 ANSI 棰滆壊 |
| `--help` | 鏍囧織 | - | 鏄剧ず甯姪淇℃伅骞堕€€鍑?|
| `--version` | 鏍囧織 | - | 鏄剧ず鐗堟湰淇℃伅骞堕€€鍑?|

### 甯姪杈撳嚭绾﹀畾

- 鏍瑰懡浠ゅ悕锛歚kb`
- 鏍瑰懡浠?Usage锛歚kb [OPTIONS] <COMMAND>`
- `kb -h` 浼氬湪甯歌鍙傛暟璇存槑鍚庨檮鍔犲垎缁勫懡浠ゆ瑙堬紝渚夸簬蹇€熷彂鐜板父鐢ㄥ瓙鍛戒护銆?- `kb --help` 瀹屾暣杈撳嚭鍙楀揩鐓ф祴璇曚繚鎶わ細`crates/kb-cli/tests/snapshots/cli_surface__kb_root_help_full.snap`

## 鐜鍙橀噺锛堝疄鐜帮級

| 鍙橀噺 | 璇存槑 |
|---|---|
| `KB_GLOBAL_CONFIG` | 鎸囧畾鍏ㄥ眬閰嶇疆鏂囦欢璺緞锛堢敤浜?`kb env` 涓?`init` 鐨勫叏灞€鐭ヨ瘑搴撶櫥璁帮級 |
| `KB_QUIET` | 杩愯鏃堕厤缃眰锛堢瓑浠?quiet锛?|
| `KB_VERBOSE` | 杩愯鏃堕厤缃眰锛堢瓑浠?verbose锛?|
| `KB_NO_COLOR` | 杩愯鏃堕厤缃眰锛堢瓑浠?no-color锛?|

## 杈撳嚭瑙勫垯

| 妯″紡 | 鏍囧噯杈撳嚭 (stdout) | 鏍囧噯閿欒 (stderr) |
|---|---|---|
| `text` | 渚涗汉闃呰鐨勫懡浠ゆ墽琛岀粨鏋溿€佹暟鎹垪琛ㄣ€佹彁绀轰俊鎭?| 璀﹀憡銆侀敊璇俊鎭€佹墽琛屾棩蹇楋紙鏍规嵁 verbose 绾у埆锛?|
| `json` | 浠呰緭鍑虹粨鏋勫寲鐨?JSON 缁撴灉瀵硅薄 | 鎵€鏈夋棩蹇椾笌浜虹被鍙鐨勬彁绀轰俊鎭€佽鍛娿€侀敊璇枃鏈?|

琛ュ厖绾﹀畾锛?
- text 妯″紡锛?  - 浣跨敤鍛戒护涓撳睘澶氳鏍煎紡锛堜緥濡?`init`/`add` 鐨勫叧閿瓧娈垫憳瑕併€乣list` 鐨勮〃鏍煎寲鏉＄洰銆乣check`/`update` 鐨勭粺璁℃憳瑕侊級銆?  - 涓嶄繚璇佸瓧娈甸『搴忓彲鏈哄櫒瑙ｆ瀽锛岄潰鍚戜汉宸ラ槄璇汇€?- json 妯″紡锛?  - 杈撳嚭绋冲畾 envelope锛歚schema_version + status + data`锛堟垨 `error`锛夈€?  - 鍐呴儴 text 娓叉煋瀛楁 `_raw_text` 涓嶄細鍑虹幇鍦ㄦ渶缁?JSON 杈撳嚭涓€?
### JSON 鎴愬姛鍝嶅簲绀轰緥
```json
{
  "schema_version": "1.0",
  "status": "success",
  "data": {}
}
```

### JSON 閿欒鍝嶅簲绀轰緥
```json
{
  "schema_version": "1.0",
  "error": {
    "code": "E1001",
    "message": "閰嶇疆鏂囦欢鍐欏叆澶辫触",
    "suggestion": "妫€鏌ユ枃浠舵潈闄愬拰纾佺洏绌洪棿",
    "details": {}
  }
}
```

## 鍓嶇疆鏉′欢

闄?`kb init`銆乣kb env`銆乣kb completion` 澶栵紝鍏朵粬鍛戒护鍧囪姹傚綋鍓嶇洰褰曞瓨鍦?`kb.md`銆傝嫢鏈垵濮嬪寲锛屽懡浠ゅ皢涓柇骞惰繑鍥?`E3001`锛堥€€鍑虹爜 3锛夈€?
## `.kbignore` 瑙勫垯锛圡VP锛?
> 璇︾粏璇硶涓庝氦浜掕竟鐣岃锛歚docs/cli/kbignore-spec.md`銆?
- 鏂囦欢浣嶇疆锛歚./.kb/.kbignore`
- 璇硶瀛愰泦锛?  - 绌鸿涓?`#` 娉ㄩ噴
  - 绠€鍗曢€氶厤绗︼細`*`銆乣?`
  - 鐩綍闄愬畾锛氬熬闅?`/`
- 鍛戒腑 `.kbignore` 鐨勮矾寰勪笉浼氳繘鍏ョ煡璇嗗簱绱㈠紩銆?- 褰撳墠鐗堟湰涓嶅寘鍚鏉傝鍒欙紙濡傚鏉傚弽閫夐摼锛夈€?
## 鍛戒护闆?
### `kb init`

* **璇硶锛?* `kb init [NAME] [--force]`
* **鍙傛暟锛?*
  * `NAME`锛堝彲閫夛級锛氱煡璇嗗簱鍚嶇О銆傞粯璁ゅ€间负褰撳墠鐩綍鍚嶃€?* **閫夐」锛?*
  * `--force`锛氳鐩栧凡瀛樺湪鐨勫垵濮嬪寲缁撴瀯銆?* **璇箟锛?*
  * **鎴愬姛锛?* 鍒涘缓 `kb.md` 涓?`.kb/`锛屽苟鍦?`./.kb/` 涓嬪垱寤?`.kbignore`锛堥粯璁ゅ寘鍚?`kb.md`銆乣.kb/refs.json`銆乣.kb/regs.json`銆乣.kb/index*.json`銆乣.kb/schema.version` 绛夊唴閮ㄩ厤缃枃浠跺拷鐣ヨ鍒欙級銆傝嫢浣跨敤 `--force`锛屽垯鍦?JSON 杈撳嚭涓繑鍥?`overwritten: true`銆?  * **闄勫姞琛屼负锛?* 鎴愬姛鍚庝細灏濊瘯灏嗗綋鍓嶇煡璇嗗簱鏍圭洰褰曞啓鍏ュ叏灞€娉ㄥ唽琛紙`knowledge_bases`锛夈€?    * 璺緞鏉ユ簮浼樺厛绾э細`KB_GLOBAL_CONFIG` > `~/.kb-cli/config.json`
    * 鐧昏澶辫触涓嶄細褰卞搷鏈湴鍒濆鍖栨垚鍔熺粨鏋溿€?  * **澶辫触锛?* 鏈娇鐢?`--force` 浣嗗綋鍓嶇洰褰曞凡鍒濆鍖栵紝杩斿洖 `E3002`锛堥€€鍑虹爜 3锛夈€傞厤缃枃浠跺啓鍏ュけ璐ワ紝杩斿洖 `E1001`锛堥€€鍑虹爜 1锛夈€?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `project` | string | 椤圭洰鍚嶇О |
| `path` | string | 鍒濆鍖栬矾寰?|
| `overwritten` | bool | 鏄惁瑕嗙洊锛堜粎鍦ㄦ彁渚?`--force` 鏃跺瓨鍦級 |

### `kb add`

* **璇硶锛?* `kb add <PATH> [ALIAS] [--mode <link|copy>] [--category <CATEGORY>]`
* **鍙傛暟锛?*
  * `PATH`锛堝繀濉級锛氱洰鏍囨枃浠舵垨鐩綍璺緞锛屽繀椤诲瓨鍦ㄣ€?  * `ALIAS`锛堝彲閫夛級锛氳嚜瀹氫箟鍒悕銆?* **閫夐」锛?*
  * `--mode <link|copy>`锛氭坊鍔犳ā寮忋€傞粯璁?`link`銆?    * `link`锛氫粎寤虹珛璺緞绗﹀彿绱㈠紩锛屼笉鏀瑰彉鍘熸枃浠剁粨鏋勩€?    * `copy`锛氬鍒舵枃浠跺埌 `./.kb/raw/`锛屽悗缁敱 `kb update` 鑷姩褰掓。鍒?`kb.md`銆?  * `--category <CATEGORY>`锛氬皢鏉＄洰娣诲姞鍒版寚瀹氬垎绫汇€傝嫢鍒嗙被涓嶅瓨鍦紝杩斿洖 `E2007` 骞舵彁绀哄彲閫夊垎绫汇€?* **鍒悕瑙ｆ瀽瑙勫垯锛? 姝ワ級锛?*
  1. 鑻ユ彁渚涗簡 `ALIAS` 涓斾笉涓虹┖锛屽垯浣跨敤璇ュ埆鍚嶃€?  2. 鑻ョ渷鐣?`ALIAS` 鎴栦负绌猴紝鍒欎娇鐢ㄨ鑼冨寲鍚庣殑 `PATH` 浣滀负鍒悕銆?  3. 瑙勮寖鍖栬鍒欙細缁濆璺緞淇濇寔缁濆锛涚浉瀵硅矾寰勪繚鎸佺浉瀵癸紱娑堥櫎 `./` 骞跺帇缂?`//` 涓?`/`銆?* **璇箟锛?*
  * **鎴愬姛锛?* 浣跨敤瑙ｆ瀽鍚庣殑鍒悕娉ㄥ唽璺緞寮曠敤銆?  * **鍛戒腑蹇界暐锛?* 鑻?`PATH` 鍛戒腑 `./.kb/.kbignore` 瑙勫垯锛屽垯杈撳嚭鎻愮ず骞跺拷鐣ユ湰娆℃坊鍔狅紙涓嶅啓鍏ョ储寮曪紝涓嶄綔涓洪敊璇級銆?  * **鍙椾繚鎶よ矾寰勶細** `kb.md` 涓?`./.kb/` 涓嬮櫎 `./.kb/raw/` 澶栫殑鍐呴儴鏂囦欢绂佹鍔犲叆绱㈠紩锛岃繑鍥?`E2009`銆?  * **澶辫触锛?* 璺緞涓嶅瓨鍦ㄨ繑鍥?`E5001`锛堥€€鍑虹爜 5锛夛紱鍒悕鍐茬獊杩斿洖 `E2003`锛堥€€鍑虹爜 2锛屾彁绀虹敤鎴烽噸鏂版寚瀹氭垨鍙栨秷锛夛紱璺緞宸蹭娇鐢ㄥ叾浠栧埆鍚嶆敞鍐岃繑鍥?`E2006`锛堥€€鍑虹爜 2锛夛紱鍒悕鍖呭惈闈炴硶瀛楃杩斿洖 `E2001`锛堥€€鍑虹爜 2锛夛紱鎸囧畾鍒嗙被涓嶅瓨鍦ㄨ繑鍥?`E2007`锛堥€€鍑虹爜 2锛夈€?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `alias` | string | 瑙ｆ瀽鍚庣殑鍒悕 |
| `path` | string | 鐩爣璺緞 |

### `kb mv`

* **璇硶锛?* `kb mv <ALIAS> <NEW_ALIAS>`
* **鍙傛暟锛?*
  * `ALIAS`锛堝繀濉級锛氬綋鍓嶅埆鍚嶏紝蹇呴』瀛樺湪銆?  * `NEW_ALIAS`锛堝繀濉級锛氱洰鏍囧埆鍚嶏紝蹇呴』涓嶅瓨鍦ㄣ€?* **璇箟锛?*
  * **鎴愬姛锛?* 閲嶅懡鍚嶅埆鍚嶏紝鐗╃悊璺緞鍜屾潈闄愪繚鎸佷笉鍙樸€?  * **澶辫触锛?* 婧愬埆鍚嶄笉瀛樺湪杩斿洖 `E5002`锛堥€€鍑虹爜 5锛夛紱鐩爣鍒悕宸插瓨鍦ㄨ繑鍥?`E2003`锛堥€€鍑虹爜 2锛夛紱鐩爣鍒悕涓嶅悎娉曡繑鍥?`E2001`锛堥€€鍑虹爜 2锛夈€?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `old_alias` | string | 鍘熷埆鍚?|
| `new_alias` | string | 鏂板埆鍚?|

### `kb rm`

* **璇硶锛?* `kb rm <ALIAS> [--force]`
* **鍙傛暟锛?*
  * `ALIAS`锛堝繀濉級锛氬緟绉婚櫎鐨勫埆鍚嶏紝蹇呴』瀛樺湪銆?* **閫夐」锛?*
  * `--force`锛氫繚鐣欏弬鏁帮紙褰撳墠瀹炵幇涓嶅尯鍒嗘槸鍚︿紶鍏ヨ鍙傛暟锛夈€?* **浜や簰琛屼负锛?* 褰撳墠鐗堟湰鏃犱氦浜掔‘璁ゆ祦绋嬶紝鍛戒护浼氱洿鎺ユ墽琛屸€滅Щ闄ゅ紩鐢ㄢ€濄€?* **璇箟锛?*
  * **鎴愬姛锛?* 绉婚櫎鍒悕寮曠敤锛?*涓嶄細**鍒犻櫎瀹為檯鐗╃悊鏂囦欢銆?  * **澶辫触锛?* 鍒悕涓嶅瓨鍦ㄨ繑鍥?`E5002`锛堥€€鍑虹爜 5锛夈€?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `removed_alias` | string | 琚Щ闄ょ殑鍒悕 |
| `path` | string | 鍘熷叧鑱旂殑鐗╃悊璺緞 |

### `kb path`

* **璇硶锛?* `kb path <ALIAS> [NEW_PATH]`
* **鍙傛暟锛?*
  * `ALIAS`锛堝繀濉級锛氱洰鏍囧埆鍚嶏紝蹇呴』瀛樺湪銆?  * `NEW_PATH`锛堝彲閫夛級锛氭柊鐗╃悊璺緞銆?* **璇箟锛?*
  * **鎴愬姛锛堟煡璇級锛?* 鏈彁渚?`NEW_PATH` 鏃讹紝杩斿洖褰撳墠鐨勭墿鐞嗚矾寰勩€?  * **鎴愬姛锛堟洿鏂帮級锛?* 鎻愪緵 `NEW_PATH` 鏃讹紝楠岃瘉鏂拌矾寰勫瓨鍦ㄥ悗鏇存柊寮曠敤锛岃緭鍑哄寘鍚棫璺緞銆?  * **澶辫触锛?* 鍒悕涓嶅瓨鍦ㄨ繑鍥?`E5002`锛堥€€鍑虹爜 5锛夛紱鏂拌矾寰勪笉瀛樺湪杩斿洖 `E5001`锛堥€€鍑虹爜 5锛屼粎鍦ㄦ洿鏂版ā寮忎笅锛夈€?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁锛堟煡璇級 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `alias` | string | 鍒悕 |
| `path` | string | 褰撳墠璺緞 |

| 瀛楁锛堟洿鏂帮級 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `alias` | string | 鍒悕 |
| `old_path` | string | 鏃ц矾寰?|
| `new_path` | string | 鏂拌矾寰?|

### `kb chmod`

* **璇硶锛?* `kb chmod <ALIAS> <PERMS>`
* **鍙傛暟锛?*
  * `ALIAS`锛堝繀濉級锛氱洰鏍囧埆鍚嶃€?  * `PERMS`锛堝繀濉級锛氭潈闄愬瓧绗︿覆銆?* **鏉冮檺瀛楃闆嗚〃锛?*
  * `r`锛氳鍙?  * `w`锛氬啓鍏?  * `x`锛氭墽琛?  * `d`锛氬垹闄?  * `c`锛氶厤缃?  * `-`锛氭棤鏉冮檺锛堝繀椤诲崟鐙娇鐢級
* **鏍￠獙瑙勫垯锛?* 浠呭厑璁稿寘鍚?`[r,w,x,d,c,-]`锛涘鏋滀娇鐢?`-` 鍒欎笉鑳藉寘鍚叾浠栧瓧绗︼紱瀛楃涓嶈兘閲嶅锛涚┖瀛楃涓叉棤鏁堛€?* **璇箟锛?*
  * **鎴愬姛锛?* 鏇存柊鏉冮檺锛屽苟杩斿洖鏂扮殑鏉冮檺瀛楃涓层€?  * **澶辫触锛?* 鍒悕涓嶅瓨鍦ㄨ繑鍥?`E5002`锛堥€€鍑虹爜 5锛夛紱鏃犳晥鏉冮檺瀛楃涓茶繑鍥?`E2004`锛堥€€鍑虹爜 2锛夈€?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `alias` | string | 鍒悕 |
| `permissions` | string | 鏇存柊鍚庣殑鏉冮檺瀛楃涓?|

### `kb list`

> Implementation note (2026-04-18): list rows are sourced from `./.kb/regs.json` records that still exist on disk.



* **璇硶锛?* `kb list [--long] [--tree] [--filter <PATTERN>]`
* **閫夐」锛?*
  * `--long`锛氭樉绀鸿缁嗕俊鎭紙淇敼鏃堕棿 mtime锛屽ぇ灏?size锛夈€?* `--tree`锛氫互鏍戠姸鏍煎紡杈撳嚭锛涘湪 JSON 妯″紡涓嬭繑鍥?`data.categories` 浣滀负鍒嗙被鍙戠幇鐨勬潈濞佹潵婧愩€?  * `--filter <PATTERN>`锛氫娇鐢?glob 妯″紡杩囨护缁撴灉銆?* **璇箟锛?*
  * **鎴愬姛锛?* 杩斿洖鍖归厤鐨勬潯鐩€傚垪琛ㄤ负绌?*涓?*绠椾綔閿欒銆?  * **澶辫触锛?* 鏃犳晥鐨?glob 妯″紡杩斿洖 `E2002`锛堥€€鍑虹爜 2锛夈€?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁锛堢煭鏍煎紡锛?| 绫诲瀷 | 璇存槑 |
|---|---|---|
| `items` | array | 鏉＄洰鏁扮粍锛堝寘鍚?`alias`, `path`, `perms` 瀛楁锛?|

| 瀛楁锛?-long 闄勫姞瀛楁锛?| 绫诲瀷 | 璇存槑 |
|---|---|---|
| `mtime` | string | Unix 鏃堕棿鎴崇瀛楃涓?|
| `size` | number | 鏂囦欢澶у皬锛堝瓧鑺傦級 |

| 瀛楁锛?-tree 闄勫姞瀛楁锛?| 绫诲瀷 | 璇存槑 |
|---|---|---|
| `categories` | array | 鍙敤鍒嗙被鍚嶇О鍒楄〃锛坄--category` 鐨勫€欓€夊€硷紝濡?`raw`, `docs`, `src`锛?|

### `kb ask`

* **璇硶锛?* `kb ask <QUESTION> [--top <N>]`
* **鍙傛暟锛?*
  * `QUESTION`锛堝繀濉級锛氳嚜鐒惰瑷€鏌ヨ鍐呭銆?* **閫夐」锛?*
  * `--top <N>`锛氳繑鍥炴渶鐩稿叧鐨?N 涓笂涓嬫枃锛岄粯璁ゅ€间负 1銆?* **璇箟锛?*
  * **鎴愬姛锛?* 杩斿洖妫€绱㈠埌鐨勪笂涓嬫枃涓庡崰浣嶇粨鏋勶紝`engine_status` 鍥哄畾涓?`"disabled"`锛?*涓嶄細**瑙﹀彂瀹為檯鐨?AI 鐢熸垚銆?  * **澶辫触锛?* 鏃犲彲鐢ㄤ笂涓嬫枃锛堢煡璇嗗簱涓虹┖鎴栨棤鍖归厤椤癸級杩斿洖 `E5003`锛堥€€鍑虹爜 5锛夈€?* **妫€绱笌鎽樿绠楁硶锛堝疄鐜扮害鏉燂級锛?*
  * 妫€绱㈠熀浜庢湰鍦?`IndexSnapshot.records`锛屼粎浣跨敤 `present=true` 鐨勮褰曞弬涓庢墦鍒嗐€?  * 鍙洖鎺掑簭浣跨敤纭畾鎬ц鍒欙細鍏堟寜 `score` 闄嶅簭锛屽啀鎸?`alias` 鍗囧簭锛屽啀鎸?`path` 鍗囧簭銆?  * 鎽樿浼樺厛浣跨敤鎸佷箙鍖?`excerpt` 瀛楁銆?  * 鎽樿绐楀彛閲囩敤 **query-token 鍛戒腑瀵嗗害绐楀彛**锛氬湪鍥哄畾绐楀彛闀垮害鍐呴€夋嫨 query 鍛戒腑瀵嗗害鏈€楂樼殑鐗囨銆?  * 绐楀彛鍚屽垎鏃堕噰鐢ㄦ樉寮?tie-break锛氬厛鏈€灏?`start`锛屽啀鏈€灏?`end`銆?  * 鎽樿鍒囩墖鎸?**grapheme cluster** 杈圭晫澶勭悊锛坄unicode-segmentation`锛宔xtended clusters锛夛紝閬垮厤缁勫悎瀛楃涓?ZWJ emoji 琚笉鑷劧鎴柇銆?  * 鑻ユ棤鍛戒腑绐楀彛锛屽垯鍥為€€鍒扮ǔ瀹氭埅鏂紙浠嶄繚鎸?grapheme 杈圭晫瀹夊叏锛夈€?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `engine_status` | string | 鍥哄畾杩斿洖 `"disabled"` |
| `provider` | string\|null | 鍥哄畾杩斿洖 `null` |
| `trace_id` | string\|null | 鍥哄畾杩斿洖 `null` |
| `retrieved_context` | array | 妫€绱㈠埌鐨勪笂涓嬫枃鏁扮粍锛堝寘鍚?`alias`, `snippet`, `score` 0-1 鐨勬暟瀛楋級 |

### `kb env`

* **璇硶锛?* `kb env [--list]`
* **閫夐」锛?*
  * `--list`锛氬垪鍑哄叏灞€娉ㄥ唽琛ㄤ腑鈥滀粛瀛樺湪鈥濈殑鐭ヨ瘑搴撴牴鐩綍銆?* **璇箟锛?*
  * 鏈紶 `--list` 鏃讹紝榛樿鍥為€€涓?`list(default)` 琛屼负銆?  * 璇诲彇鍏ㄥ眬閰嶇疆涓殑 `knowledge_bases`銆?  * 浠呰繑鍥炲綋鍓嶇鐩樹笂浠嶅瓨鍦紝涓斿寘鍚?`kb.md` 涓?`.kb/` 鐨勭洰褰曘€?  * 鍚屾椂杩斿洖锛?    * `recorded_total`锛氭敞鍐岃〃璁板綍鎬绘暟锛堜笉杩囨护锛?    * `count`锛氬疄闄呬粛瀛樺湪涓旀湁鏁堢殑鐭ヨ瘑搴撴暟閲?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `mode` | string | 褰撳墠鎵ц妯″紡锛坄list` 鎴?`list(default)`锛?|
| `items` | array | 鏈夋晥鐭ヨ瘑搴撴暟缁勶紝姣忛」鍖呭惈 `path` |
| `count` | number | `items` 鏁伴噺 |
| `recorded_total` | number | 娉ㄥ唽琛ㄤ腑鐨勬€昏褰曟暟 |
| `config_path` | string | 褰撳墠浣跨敤鐨勫叏灞€閰嶇疆鏂囦欢璺緞 |

### `kb check`

> Implementation note (2026-04-18): `kb check` compares declarations from `kb.md + ./.kb/raw/` against registrations in `./.kb/regs.json` (not `refs.json`).
> Reason mapping includes `content_changed`, `metadata_changed`, `deleted`, `not_found`.
> `to_add` means in declarations but not in `regs.json`; `to_remove` means in `regs.json` but no longer in `kb.md`/`raw`.


> 璇︾粏鐘舵€佹満銆佸垽瀹氫紭鍏堢骇涓庢晥鐜囩瓥鐣ヨ锛歚docs/cli/state-diff-contract.md`銆?>
> `kb.md` 鏍煎紡涓庤В鏋愮害鏉熻锛歚docs/cli/kb-md-spec.md`銆?
* **璇硶锛?* `kb check [--raw-only] [--category <CATEGORY>] [--top <N>]`
* **閫夐」锛?*
  * `--raw-only`锛氫粎妫€鏌?`path` 浠?`./.kb/raw/` 寮€澶寸殑宸叉敞鍐屾潯鐩€?  * `--category <CATEGORY>`锛氫粎杈撳嚭鎸囧畾鍒嗙被涓嬬殑宸紓銆傝嫢鍒嗙被涓嶅瓨鍦紝杩斿洖 `E2007`锛堥€€鍑虹爜 2锛夈€?  * `--top <N>`锛氶檺鍒舵瘡绉嶅樊寮傜被鍨嬭繑鍥炵殑鏉＄洰鏁帮紝榛樿 `50`銆?* **璇箟锛?*
  * 鍙仛妫€鏌ワ紝涓嶄慨鏀?`kb.md`銆佷笉鍐欏叆 `.kb/index*.json`銆?  * 褰撳墠蹇収鏉ユ簮涓?`.kb/refs.json` 鎶曞奖璁板綍锛堣€岄潪鐩綍鍏ㄩ噺鎵弿锛夈€?  * 璁＄畻宸紓鍓嶅簲鐢?`./.kb/.kbignore`锛涘懡涓鍒欑殑璺緞涓嶈繘鍏ュ樊寮傞泦鍚堛€?  * 鍙椾繚鎶よ矾寰勮繃婊わ細`kb.md` 涓?`./.kb/` 涓嬮櫎 `./.kb/raw/` 澶栫殑鍐呴儴鏂囦欢涓嶅弬涓庣储寮曞樊寮傘€?  * 杈撳嚭鈥滃緟鏂板 / 寰呮洿鏂?/ 寰呭垹闄わ紙鎴栧緟鏍囪鏃犳晥锛夆€濆垪琛紝渚涚敤鎴风‘璁ゅ悗鎵ц `kb update`銆?  * 鐘舵€佸垽瀹氬熀绾匡細浠モ€滄渶杩戜竴娆℃垚鍔熶笖闈?`--dry-run` 鐨?`kb update` 缁撴灉蹇収鈥濅负姣旇緝鍩虹嚎锛涜嫢涓嶅瓨鍦ㄥ熀绾匡紝鍒欐寜绌哄熀绾垮鐞嗐€?  * 鐘舵€佸垽瀹氶敭锛氶粯璁や娇鐢ㄨ鑼冨寲璺緞浣滀负 `doc_id`锛堣矾寰勮韩浠斤級銆傚湪璇ョ瓥鐣ヤ笅锛岄噸鍛藉悕/杩佺Щ鎸夆€滄棫璺緞缂哄け + 鏂拌矾寰勬柊澧炩€濆鐞嗭紝鑰屼笉鏄?`path_changed` 鏇存柊銆?  * 鍒ゅ畾浼樺厛绾э紙楂樷啋浣庯級锛歚invalid`锛堜笉鍙/鍐茬獊锛夆啋 `to_add` 涓?`to_remove`锛坢issing_path 瑙傛祴锛夆啋 `to_update` 涓?`unchanged`銆?  * `to_remove` 璇箟锛氳〃绀衡€滅己澶辫矾寰勫€欓€夐」鈥濓紱鏄惁鐪熸鍒犻櫎鐢?`kb update --prune` 鍐冲畾銆?  * 澶辫触锛?    * 鍒嗙被璇诲彇澶辫触锛坄kb.md` 鏃犳硶璇诲彇锛?> `E3003`锛堥€€鍑虹爜 3锛?    * 鎸囧畾鍒嗙被涓嶅瓨鍦?-> `E2007`锛堥€€鍑虹爜 2锛?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `has_changes` | bool | 鏄惁瀛樺湪浠讳綍宸紓 |
| `to_add` | array | 寰呮柊澧炴潯鐩紙`alias`, `path`, `source`锛?|
| `to_update` | array | 寰呮洿鏂版潯鐩紙`alias`, `path`, `reason`锛?|
| `to_remove` | array | 寰呭垹闄ゆ垨寰呮爣璁版棤鏁堟潯鐩紙`alias`, `path`, `reason`锛?|
| `reason` | enum | 鏋氫妇鍊硷細`content_changed`锛堥€傜敤 to_update锛夈€乣missing_path`锛堥€傜敤 to_remove锛夈€乣invalid`锛堥€傜敤 to_remove锛夈€?br>**娉ㄦ剰锛氬綋鍓嶈韩浠界瓥鐣ワ紙doc_id=path锛変笅宸茬Щ闄?`path_changed` 鏀寔銆?* |
| `suggested_command` | string | 寤鸿鎵ц鍛戒护锛堝 `kb update` 鎴?`kb update --prune`锛?|
| `duration_ms` | number | 鑰楁椂锛堟绉掞級 |

### `kb update`

> Implementation note (2026-04-18): `kb update` applies check diff into `./.kb/regs.json`, adds new registrations, removes stale/deleted/not-found registrations, and refreshes backend index artifacts.
> It also backfills new files from `./.kb/raw/` into `kb.md` under `# raw` as `[](./.kb/raw/<file>)` with deduplication.
> Compatibility: output keeps `marked_invalid` field; current implementation reports `0` in normal flow.


> 璇︾粏鐘舵€佹満銆佸垽瀹氫紭鍏堢骇涓庢晥鐜囩瓥鐣ヨ锛歚docs/cli/state-diff-contract.md`銆?>
> `kb.md` 鏍煎紡涓庤В鏋愮害鏉熻锛歚docs/cli/kb-md-spec.md`銆?
* **璇硶锛?* `kb update [--dry-run] [--prune] [--yes] [--full]`
* **閫夐」锛?*
  * `--dry-run`锛氫粎鏄剧ず灏嗗彂鐢熺殑鍙樻洿锛屼笉鍐欏叆鏂囦欢銆?  * `--prune`锛氬垹闄ゆ棤鏁?涓嶅瓨鍦ㄨ矾寰勶紱榛樿浠呮爣璁板苟鎻愮ず銆?  * `--yes`锛氫繚鐣欏弬鏁帮紙褰撳墠瀹炵幇涓嶅尯鍒嗘槸鍚︿紶鍏ヨ鍙傛暟锛夈€?  * `--full`锛氭墽琛屽叏閲忛噸寤恒€?* **璇箟锛?*
  * 褰撳墠蹇収鏉ユ簮锛歚.kb/refs.json` 鎶曞奖璁板綍锛堜粎淇濈暀瀹為檯瀛樺湪鐨勮矾寰勶級銆?  * 宸紓杩囨护锛氬簲鐢?`./.kb/.kbignore`锛涘懡涓鍒欑殑璺緞涓嶅弬涓庣储寮曠敓鎴愪笌鍙樻洿缁熻銆?  * 鍙椾繚鎶よ矾寰勮繃婊わ細`kb.md` 涓?`./.kb/` 涓嬮櫎 `./.kb/raw/` 澶栫殑鍐呴儴鏂囦欢涓嶅弬涓庣储寮曠敓鎴愪笌鍙樻洿缁熻銆?  * 杈撳嚭鍐欏叆锛堝師瀛愬寲锛夛細
    * `.kb/index.json`
    * `.kb/index.errors.json`
    * `.kb/index.meta.json`
  * 澶辫触锛?    * 鍩虹嚎绱㈠紩蹇収璇诲彇/瑙ｆ瀽澶辫触 -> `E3003`锛堥€€鍑虹爜 3锛?    * 绱㈠紩鎸佷箙鍖栧け璐?-> `E1003`锛堥€€鍑虹爜 1锛?  * 鏇存柊/鍒犻櫎鍒ゅ畾锛?    * `added`锛氬綋鍓嶅揩鐓у瓨鍦ㄣ€佸熀绾夸笉瀛樺湪銆?    * `updated`锛氬綋鍓嶅揩鐓т笌鍩虹嚎鍚屼竴 `doc_id`锛堣矾寰勶級涓嬪唴瀹圭鍚嶅彉鍖栥€?    * `marked_invalid`锛氬熀绾垮瓨鍦ㄤ絾褰撳墠缂哄け锛坄missing_path`锛夛紝涓旀湭鍚敤 `--prune`銆?    * `removed`锛氬熀绾垮瓨鍦ㄤ絾褰撳墠缂哄け锛坄missing_path`锛夛紝涓斿惎鐢?`--prune`銆?  * `--dry-run`锛氫粎杈撳嚭璁″垝锛屼笉鍐?`.kb/index*.json`锛屼篃涓嶆洿鏂板熀绾垮揩鐓с€?  * 骞傜瓑鎬э細鍦ㄨ緭鍏ユ枃浠堕泦涓庨厤缃笉鍙樻椂锛岃繛缁墽琛屼袱娆?`kb update`锛岀浜屾搴旇繑鍥?`added=0, updated=0, removed=0`銆?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `added` | number | 鏂板鏉＄洰鏁?|
| `updated` | number | 鏇存柊鏉＄洰鏁?|
| `removed` | number | 绉婚櫎鏉＄洰鏁帮紙浠?`--prune`锛?|
| `marked_invalid` | number | 鏍囪涓烘棤鏁堢殑鏉＄洰鏁?|
| `duration_ms` | number | 鑰楁椂锛堟绉掞級 |
| `full` | bool | 鏄惁涓哄叏閲忛噸寤?|
| `raw_category_created` | bool | 鏈鏄惁鍒涘缓浜?`# raw` 鍒嗙被 |

### `kb completion`

* **璇硶锛?* `kb completion <SHELL>`
* **鍙傛暟锛?*
  * `SHELL`锛堝繀濉級锛氱洰鏍?shell锛屽綋鍓嶆敮鎸?`bash`銆乣zsh`銆乣fish`銆乣powershell`銆乣elvish`銆?* **璇箟锛?*
  * **鎴愬姛锛?* 鐢熸垚瀵瑰簲 shell 鐨勮ˉ鍏ㄨ剼鏈€?  * text 妯″紡浼氱洿鎺ヨ緭鍑鸿剼鏈枃鏈紙鍙噸瀹氬悜鍒版枃浠讹級銆?  * json 妯″紡浼氳繑鍥炵粨鏋勫寲瀛楁锛歚shell` 涓?`script`銆?* **杈撳嚭瀛楁琛紙JSON 妯″紡锛夛細**

| 瀛楁 | 绫诲瀷 | 璇存槑 |
|---|---|---|
| `shell` | string | shell 鍚嶇О锛堝皬鍐欙級 |
| `script` | string | 瀹屾暣琛ュ叏鑴氭湰鍐呭 |

## 闄勫綍锛歚kb -h` 杈撳嚭绀轰緥

褰撳墠瀹炵幇涓紝鏍瑰府鍔╀负 git 椋庢牸鍒嗙粍姒傝锛屽叧閿害瀹氬涓嬶細

```text
Usage: kb [OPTIONS] <COMMAND>

These are common kb commands used in various situations:

start a working area
manage references
maintain index state
retrieve and environment
```

## 鍒悕鍛藉悕瑙勫垯

| 瑙勫垯 | 绀轰緥 | 缁撴灉 |
|---|---|---|
| 鍏佽浣跨敤瀛楁瘝銆佹暟瀛椼€乣-`銆乣_`銆乣.`銆乣/` | `my-docs`, `v2/config` | 鍚堟硶 |
| 绂佹浣跨敤绌烘牸 | `my docs` | 闈炴硶 |
| 绂佹浠?`-` 寮€澶?| `-alias` | 闈炴硶 |
| 闀垮害蹇呴』鍦?1-128 涔嬮棿 | `a` (鍚堟硶), 绌哄瓧绗︿覆 (闈炴硶) | 闀垮害杩濊 |
| 绂佹浣跨敤淇濈暀瀛?| `init`, `env`, `help` | 闈炴硶 |

杩濆弽涓婅堪浠绘剰瑙勫垯鍧囪繑鍥?`E2001`锛堥€€鍑虹爜 2锛夈€?
## 璺緞瑙勮寖鍖栬鍒?
| 杈撳叆璺緞 | 瑙勮寖鍖栫粨鏋?| 璇存槑 |
|---|---|---|
| `./docs` | `./docs` | 鐩稿璺緞缁熶竴淇濈暀 `./` 鍓嶇紑 |
| `../docs` | `./../docs` | 鐩稿璺緞缁熶竴鍔犲墠缂€锛屼繚鐣欑埗鐩綍璇箟 |
| `/home/user/docs` | `/home/user/docs` | 缁濆璺緞淇濇寔涓嶅彉 |
| `docs//src` | `./docs/src` | 鍘嬬缉 `//` 骞惰ˉ榻?`./` 鍓嶇紑 |
| `docs/./src` | `./docs/./src` | 褰撳墠瀹炵幇涓嶄細娓呯悊涓棿娈电殑 `./` |


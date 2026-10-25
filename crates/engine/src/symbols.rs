//! Owner: core area. The 문자표 (character map) catalogue shared by the desktop
//! dialog, the CLI and MCP.
//!
//! Hangul 2024 offers four tables: 사용자, 유니코드, 한글(HNC), 완성형(KS). HwpCraft
//! offers the same four plus emoji and named special characters, a keyword /
//! code-point search, and insertion with a chosen font (`insert.symbol`).
//! KS X 1001 rows are derived from the EUC-KR table (`encoding_rs`), Unicode
//! blocks from the Unicode 15 block list, HNC sets from curated lists that follow
//! Hangul's own category names.
use super::*;
use serde::{Deserialize, Serialize};

/// Which table a set belongs to (Hangul's tabs plus HwpCraft's extras).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolGroup {
    /// 한글(HNC) 문자표: curated categories.
    Hnc,
    /// 완성형(KS X 1001) 문자표: EUC-KR rows.
    Ks,
    /// 유니코드 문자표: Unicode blocks.
    Unicode,
    /// 이모지.
    Emoji,
    /// 특수 문자: named characters (spaces, dashes, marks).
    Special,
}

impl SymbolGroup {
    pub const ALL: [SymbolGroup; 5] = [
        SymbolGroup::Hnc,
        SymbolGroup::Ks,
        SymbolGroup::Unicode,
        SymbolGroup::Emoji,
        SymbolGroup::Special,
    ];
    pub fn label(self) -> &'static str {
        match self {
            SymbolGroup::Hnc => "한글(HNC) 문자표",
            SymbolGroup::Ks => "완성형(KS) 문자표",
            SymbolGroup::Unicode => "유니코드 문자표",
            SymbolGroup::Emoji => "이모지",
            SymbolGroup::Special => "특수 문자",
        }
    }
}

enum Source {
    /// Explicit characters, in display order.
    Chars(&'static str),
    /// Inclusive code-point ranges; unassigned/surrogate points are skipped.
    Ranges(&'static [(u32, u32)]),
    /// EUC-KR lead-byte rows (inclusive).
    KsRows(u8, u8),
}

struct Def {
    id: &'static str,
    group: SymbolGroup,
    label: &'static str,
    /// Search keywords (Korean and English), space separated.
    aliases: &'static str,
    source: Source,
}

/// A set as reported to callers.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolSet {
    pub id: &'static str,
    pub group: SymbolGroup,
    pub label: &'static str,
    pub count: usize,
}

/// A named special character.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecialChar {
    pub name: &'static str,
    pub ch: char,
    pub code: String,
}

/// Named characters that are invisible or easily confused (Hangul's 입력 도우미 and
/// WordCraft's Special Characters tab, with Korean names).
pub const SPECIAL: &[(&str, char)] = &[
    ("줄 바꿈 없는 공백", '\u{00A0}'),
    ("전각 공백", '\u{3000}'),
    ("반각 공백 (en)", '\u{2002}'),
    ("전각 공백 (em)", '\u{2003}'),
    ("1/4 전각 공백", '\u{2005}'),
    ("좁은 공백", '\u{2009}'),
    ("영 폭 공백", '\u{200B}'),
    ("단어 결합자", '\u{2060}'),
    ("소프트 하이픈", '\u{00AD}'),
    ("줄 바꿈 없는 하이픈", '\u{2011}'),
    ("붙임표", '\u{2010}'),
    ("줄표 (en dash)", '\u{2013}'),
    ("줄표 (em dash)", '\u{2014}'),
    ("가로줄", '\u{2015}'),
    ("말줄임표", '\u{2026}'),
    ("가운뎃점", '\u{00B7}'),
    ("작은따옴표 열기", '\u{2018}'),
    ("작은따옴표 닫기", '\u{2019}'),
    ("큰따옴표 열기", '\u{201C}'),
    ("큰따옴표 닫기", '\u{201D}'),
    ("낫표 열기", '\u{300C}'),
    ("낫표 닫기", '\u{300D}'),
    ("겹낫표 열기", '\u{300E}'),
    ("겹낫표 닫기", '\u{300F}'),
    ("저작권", '\u{00A9}'),
    ("등록 상표", '\u{00AE}'),
    ("상표", '\u{2122}'),
    ("절 기호", '\u{00A7}'),
    ("단락 기호", '\u{00B6}'),
    ("도", '\u{00B0}'),
    ("원 기호", '\u{20A9}'),
    ("원 기호 (전각)", '\u{FFE6}'),
    ("유로", '\u{20AC}'),
    ("파운드", '\u{00A3}'),
    ("엔", '\u{00A5}'),
    ("달러 (전각)", '\u{FF04}'),
    ("번호 기호", '\u{2116}'),
    ("참고 기호", '\u{203B}'),
    ("단검표", '\u{2020}'),
    ("이중 단검표", '\u{2021}'),
    ("분 (′)", '\u{2032}'),
    ("초 (″)", '\u{2033}'),
    ("퍼센트", '\u{0025}'),
    ("퍼밀", '\u{2030}'),
    ("좌우 방향 표식 (LRM)", '\u{200E}'),
    ("우좌 방향 표식 (RLM)", '\u{200F}'),
    ("바이트 순서 표식", '\u{FEFF}'),
];

const HNC_GENERAL: &str =
    "、。·‥…¨〃―∥＼∼‘’“”〔〕〈〉《》「」『』【】§※¶†‡′″‰№℡™©®℗℠℮¡¿¦¨¯´¸˘˙˚˛˝ˇ⁂⁎⁑⁕※‼⁇⁈⁉‽";
const HNC_MATH: &str = "±×÷≠≤≥∞∴∵∝∂∇≡≒≪≫√∽∫∬∮∑∏∈∋∉∌⊆⊇⊂⊃∪∩∧∨￢⇒⇔∀∃∠⊥⌒°′″∅∆∗∘∙∣∥∦≈≅≃≦≧≲≳⊕⊖⊗⊘⊙⊚⊛⊞⊟⊠⊡⊢⊣⊤⊦⊧⊨⋀⋁⋂⋃⋄⋅⋆⋈⋉⋊⋮⋯⋰⋱⟨⟩⟦⟧⟪⟫∟∡∢⊾⊿∤∧∨";
const HNC_UNITS: &str = "℃℉Å¢£¥€₩₹₽₺₴₦₱฿₫₪$¤%‰‱㎕㎖㎗ℓ㎘㏄㎣㎤㎥㎦㎙㎚㎛㎜㎝㎞㎟㎠㎡㎢㏊㎍㎎㎏㏏㎈㎉㏈㎧㎨㎰㎱㎲㎳㎴㎵㎶㎷㎸㎹㎀㎁㎂㎃㎄㎺㎻㎼㎽㎾㎿㎐㎑㎒㎓㎔Ω㏀㏁㎊㎋㎌㏖㏅㎭㎮㎯㏛㎩㎪㎫㎬㏝㏐㏓㏃㏉㏜㏆㍱㍲㍳㍴㍵㍶㍷㍸㍹㍺㏐㏑㏒㏕㏖㏗㏙㏚㏛㏜㏝㏞㏟";
const HNC_CIRCLED: &str = "①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮⑯⑰⑱⑲⑳㉑㉒㉓㉔㉕㉖㉗㉘㉙㉚㉛㉜㉝㉞㉟㊱㊲㊳㊴㊵㊶㊷㊸㊹㊺㊻㊼㊽㊾㊿⓪⓵⓶⓷⓸⓹⓺⓻⓼⓽⓾❶❷❸❹❺❻❼❽❾❿⓫⓬⓭⓮⓯⓰⓱⓲⓳⓴ⓐⓑⓒⓓⓔⓕⓖⓗⓘⓙⓚⓛⓜⓝⓞⓟⓠⓡⓢⓣⓤⓥⓦⓧⓨⓩⒶⒷⒸⒹⒺⒻⒼⒽⒾⒿⓀⓁⓂⓃⓄⓅⓆⓇⓈⓉⓊⓋⓌⓍⓎⓏ㉠㉡㉢㉣㉤㉥㉦㉧㉨㉩㉪㉫㉬㉭㉮㉯㉰㉱㉲㉳㉴㉵㉶㉷㉸㉹㉺㉻㊀㊁㊂㊃㊄㊅㊆㊇㊈㊉㊊㊋㊌㊍㊎㊏㊐㊑㊒㊓㊔㊕㊖㊗㊘㊙㊚㊛㊜㊝㊞㊟㊠㊡㊢㊣㊤㊥㊦㊧㊨㊩㊪㊫㊬㊭㊮㊯㊰㋀㋁㋂㋃㋄㋅㋆㋇㋈㋉㋊㋋";
const HNC_PAREN: &str = "⑴⑵⑶⑷⑸⑹⑺⑻⑼⑽⑾⑿⒀⒁⒂⒃⒄⒅⒆⒇⒜⒝⒞⒟⒠⒡⒢⒣⒤⒥⒦⒧⒨⒩⒪⒫⒬⒭⒮⒯⒰⒱⒲⒳⒴⒵㈀㈁㈂㈃㈄㈅㈆㈇㈈㈉㈊㈋㈌㈍㈎㈏㈐㈑㈒㈓㈔㈕㈖㈗㈘㈙㈚㈛㈜㈝㈞㈠㈡㈢㈣㈤㈥㈦㈧㈨㈩㈪㈫㈬㈭㈮㈯㈰㈱㈲㈳㈴㈵㈶㈷㈸㈹㈺㈻㈼㈽㈾㈿㉀㉁㉂㉃⒈⒉⒊⒋⒌⒍⒎⒏⒐⒑⒒⒓⒔⒕⒖⒗⒘⒙⒚⒛";
const HNC_GREEK: &str = "ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩαβγδεζηθικλμνξοπρστυφχψωςϐϑϒϕϖϰϱϵ";
const HNC_CYRILLIC: &str =
    "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюяЂЃЄЅІЇЈЉЊЋЌЎЏђѓєѕіїјљњћќўџ";
const HNC_ROMAN: &str = "ⅠⅡⅢⅣⅤⅥⅦⅧⅨⅩⅪⅫⅬⅭⅮⅯⅰⅱⅲⅳⅴⅵⅶⅷⅸⅹⅺⅻⅼⅽⅾⅿ½⅓⅔¼¾⅛⅜⅝⅞⅕⅖⅗⅘⅙⅚⅐⅑⅒↉⁄";
const HNC_SCRIPTS: &str = "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁿⁱ₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₒₓₔₕₖₗₘₙₚₛₜªº";
const HNC_LETTERLIKE: &str = "ℂℊℋℌℍℎℏℐℑℒℓℕ℘ℙℚℛℜℝℤℨℬℰℱℳℹ℀℁℅℆ℯℴℵℶℷℸ⅀⅁⅂⅃⅄ⅅⅆⅇⅈⅉ𝐀𝐁𝐂𝐃𝐄𝐅𝐆𝐇𝐈𝐉𝐊𝐋𝐌𝐍𝐎𝐏𝐐𝐑𝐒𝐓𝐔𝐕𝐖𝐗𝐘𝐙𝐚𝐛𝐜𝐝𝐞𝐟𝐠𝐡𝐢𝐣𝐤𝐥𝐦𝐧𝐨𝐩𝐪𝐫𝐬𝐭𝐮𝐯𝐰𝐱𝐲𝐳𝟎𝟏𝟐𝟑𝟒𝟓𝟔𝟕𝟖𝟗𝔸𝔹ℂ𝔻𝔼𝔽𝔾ℍ𝕀𝕁𝕂𝕃𝕄ℕ𝕆ℙℚℝ𝕊𝕋𝕌𝕍𝕎𝕏𝕐ℤ𝟘𝟙𝟚𝟛𝟜𝟝𝟞𝟟𝟠𝟡";
const HNC_FULLWIDTH: &str = "！＂＃＄％＆＇（）＊＋，－．／０１２３４５６７８９：；＜＝＞？＠ＡＢＣＤＥＦＧＨＩＪＫＬＭＮＯＰＱＲＳＴＵＶＷＸＹＺ［＼］＾＿｀ａｂｃｄｅｆｇｈｉｊｋｌｍｎｏｐｑｒｓｔｕｖｗｘｙｚ｛｜｝～￠￡￢￣￤￥￦";

/// Unicode 15 blocks shown in the 유니코드 문자표 (Korean label, first, last).
const UNICODE_BLOCKS: &[(&str, u32, u32)] = &[
    ("기본 라틴 (ASCII)", 0x0020, 0x007E),
    ("라틴-1 보충", 0x00A0, 0x00FF),
    ("라틴 확장-A", 0x0100, 0x017F),
    ("라틴 확장-B", 0x0180, 0x024F),
    ("국제 음성 기호 (IPA)", 0x0250, 0x02AF),
    ("조정 문자", 0x02B0, 0x02FF),
    ("결합 분음 기호", 0x0300, 0x036F),
    ("그리스·콥트", 0x0370, 0x03FF),
    ("키릴", 0x0400, 0x04FF),
    ("키릴 보충", 0x0500, 0x052F),
    ("아르메니아", 0x0530, 0x058F),
    ("히브리", 0x0590, 0x05FF),
    ("아랍", 0x0600, 0x06FF),
    ("시리아", 0x0700, 0x074F),
    ("타나", 0x0780, 0x07BF),
    ("데바나가리", 0x0900, 0x097F),
    ("벵골", 0x0980, 0x09FF),
    ("구르무키", 0x0A00, 0x0A7F),
    ("구자라트", 0x0A80, 0x0AFF),
    ("타밀", 0x0B80, 0x0BFF),
    ("텔루구", 0x0C00, 0x0C7F),
    ("칸나다", 0x0C80, 0x0CFF),
    ("말라얄람", 0x0D00, 0x0D7F),
    ("싱할라", 0x0D80, 0x0DFF),
    ("타이", 0x0E00, 0x0E7F),
    ("라오", 0x0E80, 0x0EFF),
    ("티베트", 0x0F00, 0x0FFF),
    ("미얀마", 0x1000, 0x109F),
    ("조지아", 0x10A0, 0x10FF),
    ("한글 자모", 0x1100, 0x11FF),
    ("에티오피아", 0x1200, 0x137F),
    ("체로키", 0x13A0, 0x13FF),
    ("캐나다 원주민 음절", 0x1400, 0x167F),
    ("오검", 0x1680, 0x169F),
    ("룬", 0x16A0, 0x16FF),
    ("크메르", 0x1780, 0x17FF),
    ("몽골", 0x1800, 0x18AF),
    ("음성 확장", 0x1D00, 0x1D7F),
    ("라틴 확장 추가", 0x1E00, 0x1EFF),
    ("그리스 확장", 0x1F00, 0x1FFF),
    ("일반 구두점", 0x2000, 0x206F),
    ("위 첨자·아래 첨자", 0x2070, 0x209F),
    ("통화 기호", 0x20A0, 0x20C0),
    ("결합 기호", 0x20D0, 0x20F0),
    ("문자꼴 기호", 0x2100, 0x214F),
    ("숫자 형태", 0x2150, 0x218B),
    ("화살표", 0x2190, 0x21FF),
    ("수학 연산자", 0x2200, 0x22FF),
    ("기술 기호", 0x2300, 0x23FF),
    ("제어 그림", 0x2400, 0x2426),
    ("광학 문자 인식", 0x2440, 0x244A),
    ("둘러싼 영숫자", 0x2460, 0x24FF),
    ("괘선 조각", 0x2500, 0x257F),
    ("블록 요소", 0x2580, 0x259F),
    ("도형", 0x25A0, 0x25FF),
    ("기타 기호", 0x2600, 0x26FF),
    ("딩뱃", 0x2700, 0x27BF),
    ("수학 기호-A", 0x27C0, 0x27EF),
    ("화살표 보충-A", 0x27F0, 0x27FF),
    ("점자", 0x2800, 0x28FF),
    ("화살표 보충-B", 0x2900, 0x297F),
    ("수학 기호-B", 0x2980, 0x29FF),
    ("수학 연산자 보충", 0x2A00, 0x2AFF),
    ("기타 기호·화살표", 0x2B00, 0x2BFF),
    ("글라골", 0x2C00, 0x2C5F),
    ("라틴 확장-C", 0x2C60, 0x2C7F),
    ("콥트", 0x2C80, 0x2CFF),
    ("키릴 확장-A", 0x2DE0, 0x2DFF),
    ("보충 구두점", 0x2E00, 0x2E5D),
    ("한중일 부수 보충", 0x2E80, 0x2EF3),
    ("강희 부수", 0x2F00, 0x2FD5),
    ("표의 문자 설명", 0x2FF0, 0x2FFB),
    ("한중일 기호·구두점", 0x3000, 0x303F),
    ("히라가나", 0x3041, 0x309F),
    ("가타카나", 0x30A0, 0x30FF),
    ("주음 부호", 0x3105, 0x312F),
    ("한글 호환 자모", 0x3131, 0x318E),
    ("한문 훈점", 0x3190, 0x319F),
    ("주음 부호 확장", 0x31A0, 0x31BF),
    ("한중일 획", 0x31C0, 0x31E3),
    ("가타카나 음성 확장", 0x31F0, 0x31FF),
    ("둘러싼 한중일 문자·월", 0x3200, 0x32FF),
    ("한중일 호환 (단위)", 0x3300, 0x33FF),
    ("한중일 통합 한자 확장-A", 0x3400, 0x4DBF),
    ("역경 육십사괘", 0x4DC0, 0x4DFF),
    ("한중일 통합 한자", 0x4E00, 0x9FFF),
    ("이", 0xA000, 0xA48F),
    ("리수", 0xA4D0, 0xA4FF),
    ("바이", 0xA500, 0xA62B),
    ("키릴 확장-B", 0xA640, 0xA69F),
    ("조정 음조 문자", 0xA700, 0xA71F),
    ("라틴 확장-D", 0xA720, 0xA7FF),
    ("한글 자모 확장-A", 0xA960, 0xA97C),
    ("자바", 0xA980, 0xA9DF),
    ("한글 음절", 0xAC00, 0xD7A3),
    ("한글 자모 확장-B", 0xD7B0, 0xD7FB),
    ("사용자 영역", 0xE000, 0xF8FF),
    ("한중일 호환 한자", 0xF900, 0xFAD9),
    ("알파벳 표현형", 0xFB00, 0xFB4F),
    ("아랍 표현형-A", 0xFB50, 0xFDFF),
    ("세로쓰기 형태", 0xFE10, 0xFE19),
    ("결합 반쪽 기호", 0xFE20, 0xFE2F),
    ("한중일 호환형", 0xFE30, 0xFE4F),
    ("작은 형태 변형", 0xFE50, 0xFE6B),
    ("아랍 표현형-B", 0xFE70, 0xFEFF),
    ("반각·전각 형태", 0xFF01, 0xFFEE),
    ("특수", 0xFFF9, 0xFFFD),
    ("선형 문자 B", 0x10000, 0x100FA),
    ("에게 숫자", 0x10100, 0x1013F),
    ("고대 그리스 숫자", 0x10140, 0x1018E),
    ("고대 기호", 0x10190, 0x101A0),
    ("파이스토스 원반", 0x101D0, 0x101FD),
    ("고트", 0x10330, 0x1034A),
    ("고대 이탈리아", 0x10300, 0x1032F),
    ("우가리트", 0x10380, 0x1039D),
    ("고대 페르시아", 0x103A0, 0x103D5),
    ("데저렛", 0x10400, 0x1044F),
    ("쇼", 0x10450, 0x1047F),
    ("키프로스 음절", 0x10800, 0x1083F),
    ("페니키아", 0x10900, 0x1091F),
    ("설형 문자", 0x12000, 0x12399),
    ("이집트 상형 문자", 0x13000, 0x1342F),
    ("비잔틴 음악 기호", 0x1D000, 0x1D0F5),
    ("음악 기호", 0x1D100, 0x1D1EA),
    ("고대 그리스 음악", 0x1D200, 0x1D245),
    ("태현경 기호", 0x1D300, 0x1D356),
    ("산가지 숫자", 0x1D360, 0x1D378),
    ("수학 영숫자 기호", 0x1D400, 0x1D7FF),
    ("마작 타일", 0x1F000, 0x1F02B),
    ("도미노 타일", 0x1F030, 0x1F093),
    ("카드", 0x1F0A0, 0x1F0F5),
    ("둘러싼 영숫자 보충", 0x1F100, 0x1F1FF),
    ("둘러싼 표의 문자 보충", 0x1F200, 0x1F251),
    ("기타 기호·그림", 0x1F300, 0x1F5FF),
    ("이모티콘", 0x1F600, 0x1F64F),
    ("장식 딩뱃", 0x1F650, 0x1F67F),
    ("교통·지도 기호", 0x1F680, 0x1F6FF),
    ("연금술 기호", 0x1F700, 0x1F77F),
    ("도형 확장", 0x1F780, 0x1F7FF),
    ("화살표 보충-C", 0x1F800, 0x1F8FF),
    ("기호·그림 보충", 0x1F900, 0x1F9FF),
    ("체스 기호", 0x1FA00, 0x1FA6F),
    ("기호·그림 확장-A", 0x1FA70, 0x1FAFF),
    ("레거시 컴퓨팅 기호", 0x1FB00, 0x1FBFF),
    ("한중일 통합 한자 확장-B", 0x20000, 0x2A6DF),
    ("한중일 통합 한자 확장-C", 0x2A700, 0x2B739),
    ("한중일 통합 한자 확장-D", 0x2B740, 0x2B81D),
    ("한중일 통합 한자 확장-E", 0x2B820, 0x2CEA1),
    ("한중일 통합 한자 확장-F", 0x2CEB0, 0x2EBE0),
    ("한중일 호환 한자 보충", 0x2F800, 0x2FA1D),
    ("한중일 통합 한자 확장-G", 0x30000, 0x3134A),
];

const EMOJI_SMILEYS: &[(u32, u32)] = &[
    (0x1F600, 0x1F64F),
    (0x1F910, 0x1F92F),
    (0x1F970, 0x1F97A),
    (0x1F9D0, 0x1F9DF),
    (0x1FAE0, 0x1FAE8),
];
const EMOJI_PEOPLE: &[(u32, u32)] = &[
    (0x1F466, 0x1F487),
    (0x1F574, 0x1F575),
    (0x1F57A, 0x1F57A),
    (0x1F645, 0x1F64F),
    (0x1F930, 0x1F93E),
    (0x1F9B0, 0x1F9B9),
    (0x1F9CD, 0x1F9CF),
    (0x1F9D1, 0x1F9DD),
    (0x1FAC0, 0x1FAC5),
];
const EMOJI_HANDS: &[(u32, u32)] = &[
    (0x1F446, 0x1F450),
    (0x1F590, 0x1F596),
    (0x1F64C, 0x1F64F),
    (0x1F91A, 0x1F91F),
    (0x1F932, 0x1F933),
    (0x1FAF0, 0x1FAF8),
    (0x270A, 0x270D),
    (0x261D, 0x261D),
];
const EMOJI_ANIMALS: &[(u32, u32)] = &[
    (0x1F400, 0x1F43F),
    (0x1F980, 0x1F9AE),
    (0x1F330, 0x1F345),
    (0x1F33C, 0x1F33F),
    (0x1FAB0, 0x1FABD),
];
const EMOJI_FOOD: &[(u32, u32)] = &[
    (0x1F345, 0x1F37F),
    (0x1F950, 0x1F96F),
    (0x1F9C0, 0x1F9CB),
    (0x1FAD0, 0x1FADB),
    (0x2615, 0x2615),
];
const EMOJI_TRAVEL: &[(u32, u32)] = &[
    (0x1F680, 0x1F6FF),
    (0x1F3D4, 0x1F3F0),
    (0x1F5FA, 0x1F5FF),
    (0x1F6F3, 0x1F6FC),
    (0x26EA, 0x26F5),
    (0x2708, 0x2708),
];
const EMOJI_ACTIVITY: &[(u32, u32)] = &[
    (0x1F3A0, 0x1F3CA),
    (0x1F3CF, 0x1F3D3),
    (0x1F93A, 0x1F93F),
    (0x1F945, 0x1F94F),
    (0x26BD, 0x26BE),
    (0x1F3BD, 0x1F3BF),
];
const EMOJI_OBJECTS: &[(u32, u32)] = &[
    (0x1F4A0, 0x1F4FF),
    (0x1F50A, 0x1F53D),
    (0x1F56F, 0x1F573),
    (0x1F576, 0x1F579),
    (0x1F587, 0x1F58D),
    (0x1F5A4, 0x1F5A5),
    (0x1F5B1, 0x1F5B2),
    (0x1F5C2, 0x1F5D3),
    (0x1F5DC, 0x1F5DE),
    (0x1F5E1, 0x1F5E3),
    (0x1F9E0, 0x1F9FF),
    (0x1FA70, 0x1FA7C),
    (0x1FA80, 0x1FA88),
    (0x1FA90, 0x1FABF),
];
const EMOJI_SYMBOLS: &[(u32, u32)] = &[
    (0x1F4AF, 0x1F4AF),
    (0x1F4F3, 0x1F4FF),
    (0x1F500, 0x1F53D),
    (0x1F54B, 0x1F54E),
    (0x1F550, 0x1F567),
    (0x1F5FB, 0x1F5FF),
    (0x2600, 0x26FF),
    (0x2700, 0x27BF),
    (0x1F170, 0x1F19A),
    (0x1F201, 0x1F251),
    (0x1F7E0, 0x1F7EB),
];
const EMOJI_FLAGS: &[(u32, u32)] = &[
    (0x1F1E6, 0x1F1FF),
    (0x1F3F3, 0x1F3F4),
    (0x1F6A9, 0x1F6A9),
    (0x1F38C, 0x1F38C),
];

const DEFS: &[Def] = &[
    Def {
        id: "hnc.general",
        group: SymbolGroup::Hnc,
        label: "일반 기호",
        aliases: "기호 구두점 문장부호 punctuation symbol 따옴표 낫표 참고 ※",
        source: Source::Chars(HNC_GENERAL),
    },
    Def {
        id: "hnc.math",
        group: SymbolGroup::Hnc,
        label: "수학 기호",
        aliases: "수학 math 연산 집합 적분 합 곱 근호 부등호 plus minus",
        source: Source::Chars(HNC_MATH),
    },
    Def {
        id: "hnc.units",
        group: SymbolGroup::Hnc,
        label: "단위 기호",
        aliases: "단위 unit 온도 통화 화폐 돈 원 달러 유로 미터 그램 리터 currency",
        source: Source::Chars(HNC_UNITS),
    },
    Def {
        id: "hnc.circled",
        group: SymbolGroup::Hnc,
        label: "원 문자",
        aliases: "원문자 원 숫자 동그라미 circled number 번호 ① ㉠ ㉮",
        source: Source::Chars(HNC_CIRCLED),
    },
    Def {
        id: "hnc.paren",
        group: SymbolGroup::Hnc,
        label: "괄호 문자",
        aliases: "괄호 parenthesized 번호 ⑴ ㈀ ㈎ ㈜",
        source: Source::Chars(HNC_PAREN),
    },
    Def {
        id: "hnc.box",
        group: SymbolGroup::Hnc,
        label: "괘선 조각",
        aliases: "괘선 선 표 box drawing line 테두리",
        source: Source::Ranges(&[(0x2500, 0x257F), (0x2580, 0x259F)]),
    },
    Def {
        id: "hnc.shapes",
        group: SymbolGroup::Hnc,
        label: "도형 기호",
        aliases: "도형 별 원 네모 세모 shape star circle square triangle ★ ☆ ○ ● □ ■ △ ▲",
        source: Source::Ranges(&[(0x25A0, 0x25FF), (0x2B1B, 0x2B2F), (0x2B50, 0x2B59)]),
    },
    Def {
        id: "hnc.arrows",
        group: SymbolGroup::Hnc,
        label: "화살표",
        aliases: "화살표 arrow 방향 → ← ↑ ↓ ⇒",
        source: Source::Ranges(&[
            (0x2190, 0x21FF),
            (0x27F0, 0x27FF),
            (0x2900, 0x297F),
            (0x2B00, 0x2B11),
        ]),
    },
    Def {
        id: "hnc.misc",
        group: SymbolGroup::Hnc,
        label: "기타 기호",
        aliases: "기타 날씨 카드 체스 음악 음표 하트 별자리 전화 ☎ ☏ ♨ ♪ ♥ misc",
        source: Source::Ranges(&[(0x2600, 0x26FF)]),
    },
    Def {
        id: "hnc.dingbats",
        group: SymbolGroup::Hnc,
        label: "장식 기호(딩뱃)",
        aliases: "체크 ✓ ✔ ✗ 가위 연필 꽃 별 dingbat check",
        source: Source::Ranges(&[(0x2700, 0x27BF)]),
    },
    Def {
        id: "hnc.greek",
        group: SymbolGroup::Hnc,
        label: "그리스 문자",
        aliases: "그리스 greek 알파 베타 감마 델타 파이 시그마 오메가 α β π Σ Ω",
        source: Source::Chars(HNC_GREEK),
    },
    Def {
        id: "hnc.cyrillic",
        group: SymbolGroup::Hnc,
        label: "키릴 문자",
        aliases: "키릴 러시아 cyrillic russian",
        source: Source::Chars(HNC_CYRILLIC),
    },
    Def {
        id: "hnc.hiragana",
        group: SymbolGroup::Hnc,
        label: "히라가나",
        aliases: "히라가나 일본어 가나 japanese hiragana",
        source: Source::Ranges(&[(0x3041, 0x3096), (0x309B, 0x309F)]),
    },
    Def {
        id: "hnc.katakana",
        group: SymbolGroup::Hnc,
        label: "가타카나",
        aliases: "가타카나 일본어 japanese katakana",
        source: Source::Ranges(&[(0x30A1, 0x30FA), (0x30FB, 0x30FF), (0x31F0, 0x31FF)]),
    },
    Def {
        id: "hnc.roman",
        group: SymbolGroup::Hnc,
        label: "로마 숫자·분수",
        aliases: "로마 숫자 분수 roman fraction Ⅰ Ⅱ ½ ¼",
        source: Source::Chars(HNC_ROMAN),
    },
    Def {
        id: "hnc.scripts",
        group: SymbolGroup::Hnc,
        label: "첨자",
        aliases: "첨자 위첨자 아래첨자 superscript subscript 제곱 ²",
        source: Source::Chars(HNC_SCRIPTS),
    },
    Def {
        id: "hnc.letterlike",
        group: SymbolGroup::Hnc,
        label: "영문 서체 기호",
        aliases: "서체 letterlike 수학 알파벳 굵은 double struck ℃ ℓ ℝ",
        source: Source::Chars(HNC_LETTERLIKE),
    },
    Def {
        id: "hnc.jamo",
        group: SymbolGroup::Hnc,
        label: "한글 자모",
        aliases: "한글 자모 자음 모음 옛한글 jamo ㄱ ㅏ",
        source: Source::Ranges(&[(0x3131, 0x318E)]),
    },
    Def {
        id: "hnc.jamo_old",
        group: SymbolGroup::Hnc,
        label: "옛한글 자모(첫·가운데·끝소리)",
        aliases: "옛한글 자모 초성 중성 종성 old hangul jamo",
        source: Source::Ranges(&[
            (0x1100, 0x11FF),
            (0xA960, 0xA97C),
            (0xD7B0, 0xD7C6),
            (0xD7CB, 0xD7FB),
        ]),
    },
    Def {
        id: "hnc.ipa",
        group: SymbolGroup::Hnc,
        label: "발음 기호(IPA)",
        aliases: "발음 기호 음성 phonetic ipa",
        source: Source::Ranges(&[(0x0250, 0x02AF), (0x02B0, 0x02FF)]),
    },
    Def {
        id: "hnc.fullwidth",
        group: SymbolGroup::Hnc,
        label: "전각 영문·숫자",
        aliases: "전각 반각 fullwidth halfwidth 영문 숫자",
        source: Source::Chars(HNC_FULLWIDTH),
    },
    Def {
        id: "hnc.halfwidth_kana",
        group: SymbolGroup::Hnc,
        label: "반각 가나",
        aliases: "반각 가타카나 halfwidth katakana",
        source: Source::Ranges(&[(0xFF61, 0xFF9F)]),
    },
    Def {
        id: "hnc.cjk_punct",
        group: SymbolGroup::Hnc,
        label: "한중일 기호·구두점",
        aliases: "한중일 cjk 구두점 괄호 〔 〕 《 》 〒 〇",
        source: Source::Ranges(&[(0x3001, 0x303F)]),
    },
    Def {
        id: "hnc.cjk_compat",
        group: SymbolGroup::Hnc,
        label: "한중일 호환 단위·월",
        aliases: "단위 월 요일 ㍻ ㍿ ㎝ ㎏ cjk compatibility",
        source: Source::Ranges(&[(0x3200, 0x32FF), (0x3300, 0x33FF)]),
    },
    Def {
        id: "hnc.radicals",
        group: SymbolGroup::Hnc,
        label: "한자 부수",
        aliases: "한자 부수 radical kangxi 강희",
        source: Source::Ranges(&[(0x2F00, 0x2FD5), (0x2E80, 0x2EF3)]),
    },
    Def {
        id: "hnc.hexagram",
        group: SymbolGroup::Hnc,
        label: "주역 괘",
        aliases: "주역 팔괘 육십사괘 hexagram trigram ☰",
        source: Source::Ranges(&[(0x2630, 0x2637), (0x4DC0, 0x4DFF)]),
    },
    Def {
        id: "hnc.braille",
        group: SymbolGroup::Hnc,
        label: "점자",
        aliases: "점자 braille",
        source: Source::Ranges(&[(0x2800, 0x28FF)]),
    },
    Def {
        id: "hnc.music",
        group: SymbolGroup::Hnc,
        label: "음악 기호",
        aliases: "음악 음표 악보 ♩ ♪ ♫ ♬ ♭ ♯ music note",
        source: Source::Ranges(&[(0x2669, 0x266F), (0x1D100, 0x1D1EA)]),
    },
    Def {
        id: "hnc.currency",
        group: SymbolGroup::Hnc,
        label: "통화 기호",
        aliases: "통화 화폐 돈 currency ₩ € £ ¥ $",
        source: Source::Ranges(&[
            (0x20A0, 0x20C0),
            (0x00A2, 0x00A5),
            (0xFFE0, 0xFFE1),
            (0xFFE5, 0xFFE6),
        ]),
    },
    Def {
        id: "hnc.technical",
        group: SymbolGroup::Hnc,
        label: "기술·키보드 기호",
        aliases: "기술 키보드 technical ⌘ ⌥ ⏎ ⌫ ⏻ 전원",
        source: Source::Ranges(&[(0x2300, 0x23FF), (0x2400, 0x2426)]),
    },
    Def {
        id: "ks.symbols",
        group: SymbolGroup::Ks,
        label: "KS 특수 문자 (1–12행)",
        aliases: "완성형 ks 특수문자 기호 euc-kr",
        source: Source::KsRows(0xA1, 0xAC),
    },
    Def {
        id: "ks.hangul",
        group: SymbolGroup::Ks,
        label: "KS 한글",
        aliases: "완성형 한글 2350 ks hangul",
        source: Source::KsRows(0xB0, 0xC8),
    },
    Def {
        id: "ks.hanja",
        group: SymbolGroup::Ks,
        label: "KS 한자",
        aliases: "완성형 한자 4888 ks hanja",
        source: Source::KsRows(0xCA, 0xFD),
    },
    Def {
        id: "emoji.smileys",
        group: SymbolGroup::Emoji,
        label: "표정",
        aliases: "이모지 이모티콘 표정 웃음 emoji smiley face 😀",
        source: Source::Ranges(EMOJI_SMILEYS),
    },
    Def {
        id: "emoji.people",
        group: SymbolGroup::Emoji,
        label: "사람",
        aliases: "이모지 사람 직업 가족 people person 👩",
        source: Source::Ranges(EMOJI_PEOPLE),
    },
    Def {
        id: "emoji.hands",
        group: SymbolGroup::Emoji,
        label: "손·몸짓",
        aliases: "이모지 손 몸짓 hand gesture 👍 👋",
        source: Source::Ranges(EMOJI_HANDS),
    },
    Def {
        id: "emoji.animals",
        group: SymbolGroup::Emoji,
        label: "동물·자연",
        aliases: "이모지 동물 식물 자연 animal nature 🐶 🌸",
        source: Source::Ranges(EMOJI_ANIMALS),
    },
    Def {
        id: "emoji.food",
        group: SymbolGroup::Emoji,
        label: "음식",
        aliases: "이모지 음식 음료 food drink 🍎 ☕",
        source: Source::Ranges(EMOJI_FOOD),
    },
    Def {
        id: "emoji.travel",
        group: SymbolGroup::Emoji,
        label: "여행·장소",
        aliases: "이모지 여행 교통 장소 travel place 🚗 ✈",
        source: Source::Ranges(EMOJI_TRAVEL),
    },
    Def {
        id: "emoji.activity",
        group: SymbolGroup::Emoji,
        label: "활동",
        aliases: "이모지 활동 운동 스포츠 activity sport ⚽",
        source: Source::Ranges(EMOJI_ACTIVITY),
    },
    Def {
        id: "emoji.objects",
        group: SymbolGroup::Emoji,
        label: "사물",
        aliases: "이모지 사물 물건 object 💡 📱",
        source: Source::Ranges(EMOJI_OBJECTS),
    },
    Def {
        id: "emoji.symbols",
        group: SymbolGroup::Emoji,
        label: "기호",
        aliases: "이모지 기호 symbol ❤ ✅ ⚠",
        source: Source::Ranges(EMOJI_SYMBOLS),
    },
    Def {
        id: "emoji.flags",
        group: SymbolGroup::Emoji,
        label: "깃발",
        aliases: "이모지 깃발 국기 flag 🇰🇷",
        source: Source::Ranges(EMOJI_FLAGS),
    },
];

/// Code points that are assigned in at least some Unicode 15 font: surrogates and
/// non-characters are skipped; unassigned points inside blocks are left to the
/// caller's glyph check.
fn usable(cp: u32) -> Option<char> {
    if (0xD800..=0xDFFF).contains(&cp) || (cp & 0xFFFE) == 0xFFFE || (0xFDD0..=0xFDEF).contains(&cp)
    {
        return None;
    }
    char::from_u32(cp).filter(|c| !c.is_control())
}

fn range_chars(ranges: &[(u32, u32)]) -> Vec<char> {
    // Curated range lists may overlap; keep the first occurrence.
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (a, b) in ranges {
        out.extend((*a..=*b).filter_map(usable).filter(|c| seen.insert(*c)));
    }
    out
}

/// Characters of an EUC-KR row range (KS X 1001), in table order.
fn ks_rows(first: u8, last: u8) -> Vec<char> {
    let mut out = Vec::new();
    for lead in first..=last {
        for trail in 0xA1u8..=0xFE {
            let bytes = [lead, trail];
            let (text, _, had_errors) = encoding_rs::EUC_KR.decode(&bytes);
            if had_errors {
                continue;
            }
            let mut chars = text.chars();
            if let (Some(c), None) = (chars.next(), chars.next())
                && c != '\u{FFFD}'
            {
                out.push(c);
            }
        }
    }
    out
}

/// The EUC-KR (KS X 1001) bytes of a character, when it has them.
pub fn ks_code(c: char) -> Option<[u8; 2]> {
    let mut buf = [0u8; 4];
    let s = c.encode_utf8(&mut buf);
    let (bytes, _, unmappable) = encoding_rs::EUC_KR.encode(s);
    if unmappable || bytes.len() != 2 {
        return None;
    }
    Some([bytes[0], bytes[1]])
}

fn def(id: &str) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.id == id)
}

fn chars_of(d: &Def) -> Vec<char> {
    match &d.source {
        Source::Chars(s) => {
            let mut seen = std::collections::BTreeSet::new();
            s.chars().filter(|c| seen.insert(*c)).collect()
        }
        Source::Ranges(r) => range_chars(r),
        Source::KsRows(a, b) => ks_rows(*a, *b),
    }
}

/// Every set, in table order: HNC, KS, Unicode blocks, emoji, special.
pub fn symbol_sets() -> Vec<SymbolSet> {
    let mut out: Vec<SymbolSet> = DEFS
        .iter()
        .map(|d| SymbolSet {
            id: d.id,
            group: d.group,
            label: d.label,
            count: chars_of(d).len(),
        })
        .collect();
    out.extend(UNICODE_BLOCKS.iter().map(|(label, a, b)| SymbolSet {
        id: unicode_block_id(*a),
        group: SymbolGroup::Unicode,
        label,
        count: (*a..=*b).filter(|cp| usable(*cp).is_some()).count(),
    }));
    out.push(SymbolSet {
        id: "special",
        group: SymbolGroup::Special,
        label: "특수 문자 (이름 있는 문자)",
        count: SPECIAL.len(),
    });
    out
}

/// Unicode block set ids are `u.<first code point hex>`; the strings live forever.
fn unicode_block_id(first: u32) -> &'static str {
    static IDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let ids = IDS.get_or_init(|| {
        UNICODE_BLOCKS
            .iter()
            .map(|(_, a, _)| format!("u.{a:04x}"))
            .collect()
    });
    UNICODE_BLOCKS
        .iter()
        .position(|(_, a, _)| *a == first)
        .and_then(|i| ids.get(i))
        .map(String::as_str)
        .unwrap_or("u.0000")
}

/// Characters of a set, or `None` for an unknown id.
pub fn symbol_chars(id: &str) -> Option<Vec<char>> {
    if let Some(d) = def(id) {
        return Some(chars_of(d));
    }
    if id == "special" {
        return Some(SPECIAL.iter().map(|(_, c)| *c).collect());
    }
    let hex = id.strip_prefix("u.")?;
    let first = u32::from_str_radix(hex, 16).ok()?;
    UNICODE_BLOCKS
        .iter()
        .find(|(_, a, _)| *a == first)
        .map(|(_, a, b)| range_chars(&[(*a, *b)]))
}

/// The named special characters with their `U+XXXX` codes.
pub fn special_chars() -> Vec<SpecialChar> {
    SPECIAL
        .iter()
        .map(|(name, c)| SpecialChar {
            name,
            ch: *c,
            code: format!("U+{:04X}", *c as u32),
        })
        .collect()
}

/// The Unicode block (label) a character belongs to, if listed.
pub fn block_of(c: char) -> Option<&'static str> {
    let cp = c as u32;
    UNICODE_BLOCKS
        .iter()
        .find(|(_, a, b)| (*a..=*b).contains(&cp))
        .map(|(l, _, _)| *l)
}

/// What a search found: characters to show first, then whole sets that matched.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolSearch {
    pub chars: Vec<char>,
    pub sets: Vec<SymbolSet>,
}

/// Search by code point (`U+2605`, `2605`, `#9733`), by a typed character, by a
/// special-character name, or by set keywords (Korean or English).
pub fn symbol_search(query: &str) -> SymbolSearch {
    let q = query.trim();
    let mut out = SymbolSearch::default();
    if q.is_empty() {
        return out;
    }
    let lower = q.to_lowercase();
    // Code points.
    let hex = lower
        .strip_prefix("u+")
        .or_else(|| lower.strip_prefix("0x"))
        .unwrap_or(&lower);
    if let Some(dec) = lower.strip_prefix('#')
        && let Ok(cp) = dec.parse::<u32>()
        && let Some(c) = usable(cp)
    {
        out.chars.push(c);
    } else if hex.len() >= 2
        && hex.len() <= 6
        && hex.chars().all(|c| c.is_ascii_hexdigit())
        && let Ok(cp) = u32::from_str_radix(hex, 16)
        && let Some(c) = usable(cp)
    {
        out.chars.push(c);
    }
    // Typed symbols (not letters, digits, spaces or ASCII punctuation) are looked up
    // directly; Korean or Latin words are keywords, not characters.
    for c in q.chars() {
        if !c.is_alphanumeric()
            && !c.is_whitespace()
            && !c.is_ascii_punctuation()
            && !out.chars.contains(&c)
        {
            out.chars.push(c);
        }
    }
    // Special-character names.
    for (name, c) in SPECIAL {
        if name.to_lowercase().contains(&lower) && !out.chars.contains(c) {
            out.chars.push(*c);
        }
    }
    // Set labels and aliases, every query word must match something.
    let words: Vec<&str> = lower.split_whitespace().collect();
    for set in symbol_sets() {
        let aliases = def(set.id).map(|d| d.aliases).unwrap_or("");
        let hay = format!("{} {} {}", set.label, aliases, set.id).to_lowercase();
        if words.iter().all(|w| hay.contains(w)) {
            out.sets.push(set);
        }
    }
    out.chars.truncate(256);
    out
}

/// Parameters of `insert.symbol`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolInsert {
    /// One or more characters; no line breaks or control characters.
    pub text: String,
    /// Font family applied to the inserted characters only (e.g. a symbol font).
    #[serde(default)]
    pub font: Option<String>,
}

impl Session {
    /// Insert characters at the caret (replacing the selection), optionally in a
    /// given font, as one undo step. Returns the inserted scalar range.
    pub fn insert_symbol(&mut self, p: &SymbolInsert) -> Result<Range<usize>> {
        if p.text.is_empty() {
            bail!("넣을 문자를 선택하세요.");
        }
        if p.text.chars().count() > 1000 || p.text.chars().any(|c| c.is_control()) {
            bail!("문자표는 제어 문자 없이 1,000자까지 한 번에 넣을 수 있습니다.");
        }
        if let Some(font) = &p.font
            && (font.trim().is_empty() || font.len() > 255)
        {
            bail!("글꼴 이름이 올바르지 않습니다.");
        }
        self.guard_protected()?;
        let props = match &p.font {
            Some(font) => Some(
                self.character_parameters(json!({ "fontFamily": font }))?
                    .to_string(),
            ),
            None => None,
        };
        let mut inserted = 0..0;
        self.transaction(|s| {
            s.delete_selection()?;
            let start = s.cursor.offset;
            s.insert(&p.text)?;
            let c = s.cursor;
            inserted = start..c.offset;
            if let Some(props) = &props {
                let r = if let Some(t) = c.cell {
                    s.core.apply_char_format_in_cell_native(
                        c.section,
                        c.paragraph,
                        t.control,
                        t.index,
                        t.paragraph,
                        start,
                        c.offset,
                        props,
                    )
                } else {
                    s.core
                        .apply_char_format_native(c.section, c.paragraph, start, c.offset, props)
                };
                r.map_err(err)?;
            }
            s.clear_selection();
            Ok(())
        })?;
        Ok(inserted)
    }
}

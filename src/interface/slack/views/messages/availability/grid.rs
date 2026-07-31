//! GPUの埋まり具合を表す記号の表
//!
//! サーバーを行、デバイス番号を列に取り、1台ぶんを1文字で示します。
//! 台数が増えても縦に伸びず、どの番号が空いているかが番号を数えずに読めます。
//!
//! # 記号にASCIIを使う理由
//!
//! 桁を揃えるためこの表はコードブロックに置きます。○●のような記号は
//! 文字幅が環境によって変わり（East Asian Ambiguous）、桁がずれます。
//! 表の中はASCIIだけで組み、日本語の凡例はコードブロックの外に置きます。

/// 空きを表す記号
pub const FREE_MARK: &str = ".";
/// 使用中を表す記号
pub const BUSY_MARK: &str = "#";
/// 行の先頭に置く列の見出し
const HEADER_LABEL: &str = "GPU";
/// サーバー名の後ろに空ける桁数
const NAME_PADDING: usize = 2;
/// デバイス1台あたりの桁数（記号と番号の両方がこの幅に収まる）
const COLUMN_WIDTH: usize = 3;

/// 1台のGPUの状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device {
    /// デバイス番号
    pub number: u32,
    /// 指定時刻に空いているか
    pub is_free: bool,
}

/// 1台のサーバーとそのデバイス
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    /// サーバー名
    pub name: String,
    /// デバイス（設定順）
    pub devices: Vec<Device>,
}

/// 記号の表を組み立てる
///
/// 列はすべてのサーバーのデバイス番号の和集合。あるサーバーに存在しない番号の欄は
/// 空ける。サーバーが1台もなければ`None`。
pub fn render(servers: &[Server]) -> Option<String> {
    if servers.is_empty() {
        return None;
    }

    let columns = columns_of(servers);
    let name_width = name_width_of(servers);

    let mut lines = vec![row(HEADER_LABEL, name_width, &columns, |column| {
        column.to_string()
    })];

    for server in servers {
        lines.push(row(&server.name, name_width, &columns, |column| {
            server
                .devices
                .iter()
                .find(|device| device.number == *column)
                .map(|device| if device.is_free { FREE_MARK } else { BUSY_MARK })
                .unwrap_or(" ")
                .to_string()
        }));
    }

    Some(lines.join("\n"))
}

/// 表の列に並べるデバイス番号（和集合、昇順）
fn columns_of(servers: &[Server]) -> Vec<u32> {
    let mut columns: Vec<u32> = servers
        .iter()
        .flat_map(|server| server.devices.iter().map(|device| device.number))
        .collect();
    columns.sort_unstable();
    columns.dedup();
    columns
}

/// サーバー名の欄の桁数
///
/// サーバー名がASCIIでない場合、文字数と表示幅が一致せず桁がずれる。
/// サーバー名は`nvidia-smi`が返すホスト名に由来するためASCIIとして扱う。
fn name_width_of(servers: &[Server]) -> usize {
    servers
        .iter()
        .map(|server| server.name.chars().count())
        .chain(std::iter::once(HEADER_LABEL.chars().count()))
        .max()
        .unwrap_or_else(|| HEADER_LABEL.chars().count())
        + NAME_PADDING
}

/// 表の1行を組み立てる
fn row(label: &str, name_width: usize, columns: &[u32], cell: impl Fn(&u32) -> String) -> String {
    let mut line = format!("{:<width$}", label, width = name_width);

    for column in columns {
        line.push_str(&format!("{:>width$}", cell(column), width = COLUMN_WIDTH));
    }

    line.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(name: &str, devices: &[(u32, bool)]) -> Server {
        Server {
            name: name.to_string(),
            devices: devices
                .iter()
                .map(|(number, is_free)| Device {
                    number: *number,
                    is_free: *is_free,
                })
                .collect(),
        }
    }

    #[test]
    fn no_servers_produce_no_grid() {
        assert_eq!(render(&[]), None);
    }

    #[test]
    fn each_device_shows_its_state_under_its_number() {
        let servers = vec![server(
            "gpu-server-1",
            &[(0, true), (1, false), (2, false), (3, true)],
        )];

        let grid = render(&servers).unwrap();

        let mut lines = grid.lines();
        let header = lines.next().unwrap();
        let row = lines.next().unwrap();

        for number in ["0", "1", "2", "3"] {
            assert!(header.contains(number), "列の見出しに番号が並ぶ: {grid}");
        }
        assert_eq!(
            row.matches(BUSY_MARK).count(),
            2,
            "使用中の2台が記号で示される: {grid}"
        );
        assert_eq!(
            header.find('1'),
            row.find(BUSY_MARK),
            "使用中の記号は、その番号の真下に来る: {grid}"
        );
    }

    #[test]
    fn servers_keep_the_order_they_were_given() {
        let servers = vec![
            server("gpu-server-2", &[(0, true)]),
            server("gpu-server-1", &[(0, true)]),
        ];

        let grid = render(&servers).unwrap();

        let lines: Vec<&str> = grid.lines().collect();
        assert!(lines[1].starts_with("gpu-server-2"), "{grid}");
        assert!(lines[2].starts_with("gpu-server-1"), "{grid}");
    }

    #[test]
    fn columns_cover_every_device_number_across_servers() {
        let servers = vec![
            server("small", &[(0, true), (1, true)]),
            server("large", &[(0, true), (1, true), (2, true), (3, true)]),
        ];

        let grid = render(&servers).unwrap();

        let header = grid.lines().next().unwrap();
        assert!(
            header.contains('3'),
            "台数の多いサーバーの番号まで列を用意する: {grid}"
        );
    }

    #[test]
    fn a_device_number_a_server_does_not_have_is_left_blank() {
        let servers = vec![
            server("small", &[(0, false), (1, false)]),
            server("large", &[(0, false), (1, false), (2, false), (3, false)]),
        ];

        let grid = render(&servers).unwrap();

        let small_row = grid.lines().nth(1).unwrap();
        assert_eq!(
            small_row.matches(BUSY_MARK).count(),
            2,
            "持っていない番号の欄に記号を置かない: {grid}"
        );
    }

    #[test]
    fn every_row_lines_up_with_the_header() {
        let servers = vec![
            server("a", &[(0, true), (1, false)]),
            server("a-much-longer-name", &[(0, false), (1, true)]),
        ];

        let grid = render(&servers).unwrap();

        let lines: Vec<&str> = grid.lines().collect();
        let header_positions: Vec<usize> = lines[0].match_indices('0').map(|(i, _)| i).collect();
        for line in &lines[1..] {
            let mark = line
                .char_indices()
                .find(|(_, c)| *c == '.' || *c == '#')
                .map(|(i, _)| i)
                .unwrap();
            assert_eq!(
                mark, header_positions[0],
                "サーバー名の長さが違っても記号の位置は揃う: {grid}"
            );
        }
    }

    #[test]
    fn two_digit_device_numbers_keep_the_columns_aligned() {
        let devices: Vec<(u32, bool)> = (0..12).map(|number| (number, number % 2 == 0)).collect();
        let servers = vec![server("dense", &devices)];

        let grid = render(&servers).unwrap();

        let lines: Vec<&str> = grid.lines().collect();
        assert_eq!(
            lines[0].chars().count(),
            lines[1].chars().count(),
            "2桁の番号でも見出しと行の長さが揃う: {grid}"
        );
    }
}

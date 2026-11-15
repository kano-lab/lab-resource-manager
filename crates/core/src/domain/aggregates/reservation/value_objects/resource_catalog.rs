//! 予約できるリソースの一覧と、利用者の指定からの解決

use super::resource::Resource;
use std::fmt;

/// 予約できるリソースの一覧
///
/// 何が予約できるかは研究室ごとの知識であり、利用者が名前と番号で指したリソースは
/// この一覧に照らして初めて予約の対象になる。GPUの型番のように利用者が指さない情報も、
/// この一覧が正本となる。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceCatalog {
    resources: Vec<Resource>,
}

impl ResourceCatalog {
    /// リソースを並べた順で一覧を作る
    pub fn new(resources: impl IntoIterator<Item = Resource>) -> Self {
        Self {
            resources: resources.into_iter().collect(),
        }
    }

    /// 予約できるすべてのリソース（作ったときの順）
    pub fn resources(&self) -> &[Resource] {
        &self.resources
    }

    /// 指定が指すリソースを一覧から引く
    ///
    /// # Errors
    /// 指定されたサーバー、デバイス、部屋が一覧に無い場合や、デバイスを1つも指していない場合
    pub fn select(
        &self,
        selection: &ResourceSelection,
    ) -> Result<Vec<Resource>, ResourceSelectionError> {
        match selection {
            ResourceSelection::WholeServer { server } => {
                let devices = self.devices_of(server);
                if devices.is_empty() {
                    return Err(ResourceSelectionError::UnknownServer(server.clone()));
                }
                Ok(devices.into_iter().cloned().collect())
            }
            ResourceSelection::Devices {
                server,
                device_numbers,
            } => self.select_devices(server, device_numbers),
            ResourceSelection::Room { name } => self
                .resources
                .iter()
                .find(|resource| matches!(resource, Resource::Room { name: candidate } if candidate == name))
                .map(|room| vec![room.clone()])
                .ok_or_else(|| ResourceSelectionError::UnknownRoom(name.clone())),
        }
    }

    /// いくつかの指定のどれかが指すリソースを、一覧の順に重複なく引く
    ///
    /// # Errors
    /// どれかの指定が一覧に無いものを指している場合
    pub fn select_any(
        &self,
        selections: &[ResourceSelection],
    ) -> Result<Vec<Resource>, ResourceSelectionError> {
        let mut selected = Vec::new();
        for selection in selections {
            selected.extend(self.select(selection)?);
        }

        Ok(self
            .resources
            .iter()
            .filter(|resource| selected.contains(resource))
            .cloned()
            .collect())
    }

    fn devices_of(&self, server: &str) -> Vec<&Resource> {
        self.resources
            .iter()
            .filter(|resource| matches!(resource, Resource::Gpu(gpu) if gpu.server() == server))
            .collect()
    }

    fn select_devices(
        &self,
        server: &str,
        device_numbers: &[u32],
    ) -> Result<Vec<Resource>, ResourceSelectionError> {
        if device_numbers.is_empty() {
            return Err(ResourceSelectionError::NoDeviceSelected(server.to_string()));
        }
        let devices = self.devices_of(server);
        if devices.is_empty() {
            return Err(ResourceSelectionError::UnknownServer(server.to_string()));
        }

        device_numbers
            .iter()
            .map(|&device_number| {
                devices
                    .iter()
                    .find(|resource| {
                        matches!(resource, Resource::Gpu(gpu) if gpu.device_number() == device_number)
                    })
                    .map(|&device| device.clone())
                    .ok_or_else(|| ResourceSelectionError::UnknownDevice {
                        server: server.to_string(),
                        device_number,
                    })
            })
            .collect()
    }
}

/// 利用者がリソースを指す言い方
///
/// 1つの指定が指すのは、1台のサーバーのGPUか、1つの部屋である。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceSelection {
    /// サーバーのすべてのGPU
    WholeServer {
        /// サーバー名
        server: String,
    },
    /// サーバーの、番号で指したGPU
    Devices {
        /// サーバー名
        server: String,
        /// デバイス番号（1つ以上）
        device_numbers: Vec<u32>,
    },
    /// 部屋
    Room {
        /// 部屋名
        name: String,
    },
}

impl ResourceSelection {
    /// サーバーのGPUを指す。番号が無ければそのサーバーのすべてのGPUを指す
    ///
    /// 「番号を選ばなければ全部」という入力欄から作るためのもの。
    pub fn gpus(server: impl Into<String>, device_numbers: Vec<u32>) -> Self {
        let server = server.into();
        if device_numbers.is_empty() {
            Self::WholeServer { server }
        } else {
            Self::Devices {
                server,
                device_numbers,
            }
        }
    }

    /// 部屋を指す
    pub fn room(name: impl Into<String>) -> Self {
        Self::Room { name: name.into() }
    }
}

/// 指定が予約できるリソースを指していない
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResourceSelectionError {
    /// GPUを持つサーバーとして一覧に無い
    UnknownServer(String),
    /// サーバーにそのデバイス番号のGPUが無い
    UnknownDevice {
        /// サーバー名
        server: String,
        /// デバイス番号
        device_number: u32,
    },
    /// 部屋として一覧に無い
    UnknownRoom(String),
    /// デバイスを番号で指すはずが、1つも指していない
    NoDeviceSelected(String),
}

impl fmt::Display for ResourceSelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownServer(server) => write!(f, "サーバー {server} が見つかりません"),
            Self::UnknownDevice {
                server,
                device_number,
            } => write!(
                f,
                "サーバー {server} にデバイス {device_number} が見つかりません"
            ),
            Self::UnknownRoom(name) => write!(f, "部屋 {name} が見つかりません"),
            Self::NoDeviceSelected(server) => {
                write!(f, "サーバー {server} のデバイスが指定されていません")
            }
        }
    }
}

impl std::error::Error for ResourceSelectionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::aggregates::reservation::value_objects::Gpu;

    fn gpu(server: &str, device_number: u32, model: &str) -> Resource {
        Resource::Gpu(Gpu::new(
            server.to_string(),
            device_number,
            model.to_string(),
        ))
    }

    fn room(name: &str) -> Resource {
        Resource::Room {
            name: name.to_string(),
        }
    }

    fn catalog() -> ResourceCatalog {
        ResourceCatalog::new([
            gpu("Thalys", 0, "A100"),
            gpu("Thalys", 1, "A100"),
            gpu("Freccia", 0, "RTX6000"),
            room("会議室A"),
        ])
    }

    #[test]
    fn a_whole_server_is_every_gpu_on_it() {
        assert_eq!(
            catalog().select(&ResourceSelection::gpus("Thalys", vec![])),
            Ok(vec![gpu("Thalys", 0, "A100"), gpu("Thalys", 1, "A100")])
        );
    }

    #[test]
    fn devices_come_back_in_the_order_they_were_asked_for_with_their_model() {
        assert_eq!(
            catalog().select(&ResourceSelection::gpus("Thalys", vec![1, 0])),
            Ok(vec![gpu("Thalys", 1, "A100"), gpu("Thalys", 0, "A100")])
        );
    }

    #[test]
    fn something_not_in_the_catalog_is_refused() {
        let catalog = catalog();

        assert_eq!(
            catalog.select(&ResourceSelection::gpus("Lyria", vec![])),
            Err(ResourceSelectionError::UnknownServer("Lyria".to_string()))
        );
        assert_eq!(
            catalog.select(&ResourceSelection::gpus("Thalys", vec![0, 7])),
            Err(ResourceSelectionError::UnknownDevice {
                server: "Thalys".to_string(),
                device_number: 7,
            })
        );
        assert_eq!(
            catalog.select(&ResourceSelection::room("会議室B")),
            Err(ResourceSelectionError::UnknownRoom("会議室B".to_string()))
        );
    }

    #[test]
    fn a_room_name_does_not_select_a_server_of_that_name() {
        let catalog = ResourceCatalog::new([gpu("Thalys", 0, "A100")]);

        assert_eq!(
            catalog.select(&ResourceSelection::room("Thalys")),
            Err(ResourceSelectionError::UnknownRoom("Thalys".to_string()))
        );
    }

    #[test]
    fn devices_asked_for_by_number_must_name_at_least_one() {
        let selection = ResourceSelection::Devices {
            server: "Thalys".to_string(),
            device_numbers: vec![],
        };

        assert_eq!(
            catalog().select(&selection),
            Err(ResourceSelectionError::NoDeviceSelected(
                "Thalys".to_string()
            ))
        );
    }

    #[test]
    fn several_selections_come_back_in_catalog_order_without_duplicates() {
        let selected = catalog().select_any(&[
            ResourceSelection::room("会議室A"),
            ResourceSelection::gpus("Thalys", vec![1]),
            ResourceSelection::gpus("Thalys", vec![]),
        ]);

        assert_eq!(
            selected,
            Ok(vec![
                gpu("Thalys", 0, "A100"),
                gpu("Thalys", 1, "A100"),
                room("会議室A"),
            ])
        );
    }
}

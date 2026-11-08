//! Active displays: monitor name, resolution and refresh rate (CCD API).

use veyl_core::specs::DisplayInfo;
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_TARGET_DEVICE_NAME,
    QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::ERROR_SUCCESS;

use crate::handle::from_wide;

pub fn displays() -> Vec<DisplayInfo> {
    let (mut np, mut nm) = (0u32, 0u32);
    // SAFETY: out-pointers to locals.
    if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) }
        != ERROR_SUCCESS
    {
        return Vec::new();
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
    // SAFETY: buffers sized by the query above; the counts are updated on return.
    let r = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut np,
            paths.as_mut_ptr(),
            &mut nm,
            modes.as_mut_ptr(),
            None,
        )
    };
    if r != ERROR_SUCCESS {
        return Vec::new();
    }
    paths.truncate(np as usize);
    modes.truncate(nm as usize);
    paths
        .iter()
        .filter_map(|p| {
            // SAFETY: without QDC_VIRTUAL_MODE_AWARE the union holds modeInfoIdx.
            let m = modes.get(unsafe { p.sourceInfo.Anonymous.modeInfoIdx } as usize)?;
            if m.infoType != DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
                return None;
            }
            // SAFETY: infoType selects the sourceMode member.
            let src = unsafe { m.Anonymous.sourceMode };
            let rate = p.targetInfo.refreshRate;
            let mut name = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
            name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
            name.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
            name.header.adapterId = p.targetInfo.adapterId;
            name.header.id = p.targetInfo.id;
            // SAFETY: the header describes this struct and its size.
            let named = unsafe { DisplayConfigGetDeviceInfo(&mut name.header) } == 0;
            Some(DisplayInfo {
                name: named
                    .then(|| from_wide(&name.monitorFriendlyDeviceName))
                    .filter(|n| !n.trim().is_empty()),
                width: src.width,
                height: src.height,
                refresh_hz: (rate.Denominator != 0)
                    .then(|| f64::from(rate.Numerator) / f64::from(rate.Denominator)),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_without_failing() {
        for d in super::displays() {
            assert!(d.width > 0 && d.height > 0, "{d:?}");
        }
    }
}

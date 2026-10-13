/* SPDX-License-Identifier: GPL-2.0-only OR MIT */
/* A base EDID for the built-in panel, which the DCP firmware describes without one. */
#ifndef __APPLE_PANEL_EDID_H__
#define __APPLE_PANEL_EDID_H__

#include <linux/types.h>

#define DCP_PANEL_EDID_SIZE	128

struct apple_dcp;
struct dcp_panel;
struct drm_edid;

void dcp_panel_edid_block(const struct dcp_panel *panel, const char *name,
			  u8 block[DCP_PANEL_EDID_SIZE]);
const struct drm_edid *dcp_panel_edid(struct apple_dcp *dcp);

#endif

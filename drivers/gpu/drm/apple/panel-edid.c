// SPDX-License-Identifier: GPL-2.0-only OR MIT
/*
 * A base EDID for the built-in panel.
 *
 * The DCP firmware gives no EDID for a panel: the only EDID source is the
 * DPAVSERV service, which is started for external displays only. Without one,
 * compositors see no make, model or physical size on eDP-1 (aquamarine logs
 * "failed to parse edid" and Hyprland shows an empty make and model). This
 * block gives them what the panel node knows: a name, the vendor and the size.
 *
 * It carries no detailed timing, because the panel's pixel clock (over 1 GHz
 * at 120 Hz) is beyond what an EDID timing can hold, and no range descriptor,
 * because the driver supplies the refresh range the firmware reports. The
 * primaries are sRGB's, so a compositor that reads them gets a sane gamut
 * rather than zeros. Modes keep coming from the firmware, as before.
 */

#include <linux/math.h>
#include <linux/string.h>

#include <drm/drm_edid.h>

#include "dcp-internal.h"
#include "panel-edid.h"

/* The EDID manufacturer ID "APP", the PnP ID of Apple. */
#define EDID_MFG_APPLE_0	0x06
#define EDID_MFG_APPLE_1	0x10

#define EDID_DESC_NAME		0xfc
#define EDID_DESC_DUMMY		0x10
#define EDID_DESC_TEXT_LEN	13

void dcp_panel_edid_block(const struct dcp_panel *panel, const char *name,
			  u8 block[DCP_PANEL_EDID_SIZE])
{
	static const u8 header[8] = { 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00 };
	/* The sRGB primaries and white point, as EDID encodes them. */
	static const u8 srgb_chroma[10] = { 0xee, 0x91, 0xa3, 0x54, 0x4c, 0x99,
					    0x26, 0x0f, 0x50, 0x54 };
	size_t len;
	u8 sum = 0;
	int i;

	memset(block, 0, DCP_PANEL_EDID_SIZE);
	memcpy(block, header, sizeof(header));
	block[8] = EDID_MFG_APPLE_0;
	block[9] = EDID_MFG_APPLE_1;
	/* Product code and serial number: none. Week 0, model year 2023. */
	block[17] = 2023 - 1990;
	block[18] = 1;			/* EDID 1.4 */
	block[19] = 4;
	block[20] = 0x80 | (3 << 4) | 5;	/* digital, 10 bits per colour, DisplayPort */
	block[21] = DIV_ROUND_CLOSEST(panel->width_mm, 10);	/* cm */
	block[22] = DIV_ROUND_CLOSEST(panel->height_mm, 10);
	block[23] = 220 - 100;		/* gamma 2.2 */
	block[24] = 0x04;		/* sRGB colour space; RGB 4:4:4; no DPMS states */
	memcpy(&block[25], srgb_chroma, sizeof(srgb_chroma));
	/* No established timings (35..37). The eight standard timings unused. */
	for (i = 38; i < 54; i++)
		block[i] = 0x01;
	/* Descriptor 1: the monitor name, newline-terminated and space-padded. */
	block[57] = EDID_DESC_NAME;
	len = strnlen(name, EDID_DESC_TEXT_LEN);
	memcpy(&block[59], name, len);
	if (len < EDID_DESC_TEXT_LEN) {
		block[59 + len] = '\n';
		memset(&block[60 + len], ' ', EDID_DESC_TEXT_LEN - len - 1);
	}
	/* Descriptors 2 to 4: dummy. */
	for (i = 72; i < 126; i += 18)
		block[i + 3] = EDID_DESC_DUMMY;
	block[126] = 0;			/* no extension blocks */
	for (i = 0; i < 127; i++)
		sum += block[i];
	block[127] = (u8)(0x100 - sum);
}

const struct drm_edid *dcp_panel_edid(struct apple_dcp *dcp)
{
	u8 block[DCP_PANEL_EDID_SIZE];

	if (!dcp_has_panel(dcp))
		return NULL;
	dcp_panel_edid_block(&dcp->panel, "Built-in", block);
	return drm_edid_alloc(block, sizeof(block));
}

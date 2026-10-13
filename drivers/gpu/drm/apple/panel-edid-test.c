// SPDX-License-Identifier: GPL-2.0-only OR MIT
/* The built-in panel's base EDID: well formed, with the panel's size and a name. */

#include <kunit/test.h>
#include <drm/drm_edid.h>

#include "dcp-internal.h"
#include "panel-edid.h"

static const struct dcp_panel j516_panel = { .width_mm = 346, .height_mm = 223 };

static void panel_edid_block_is_well_formed(struct kunit *test)
{
	static const u8 header[8] = { 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00 };
	u8 block[DCP_PANEL_EDID_SIZE];
	unsigned int sum = 0;
	int i;

	dcp_panel_edid_block(&j516_panel, "Built-in", block);
	KUNIT_EXPECT_MEMEQ(test, block, header, sizeof(header));
	KUNIT_EXPECT_EQ(test, block[18], 1);
	KUNIT_EXPECT_EQ(test, block[19], 4);
	for (i = 0; i < DCP_PANEL_EDID_SIZE; i++)
		sum += block[i];
	KUNIT_EXPECT_EQ(test, sum & 0xff, 0);
	KUNIT_EXPECT_EQ(test, block[126], 0);
}

static void panel_edid_block_carries_vendor_size_and_name(struct kunit *test)
{
	u8 block[DCP_PANEL_EDID_SIZE];

	dcp_panel_edid_block(&j516_panel, "Built-in", block);
	/* "APP" */
	KUNIT_EXPECT_EQ(test, block[8], 0x06);
	KUNIT_EXPECT_EQ(test, block[9], 0x10);
	/* 346x223 mm, in whole centimetres */
	KUNIT_EXPECT_EQ(test, block[21], 35);
	KUNIT_EXPECT_EQ(test, block[22], 22);
	/* digital input, the first descriptor a monitor name */
	KUNIT_EXPECT_TRUE(test, block[20] & 0x80);
	KUNIT_EXPECT_EQ(test, block[57], 0xfc);
	KUNIT_EXPECT_MEMEQ(test, &block[59], "Built-in\n    ", 13);
	/* no serial number, no timing descriptors */
	KUNIT_EXPECT_EQ(test, block[12] | block[13] | block[14] | block[15], 0);
	KUNIT_EXPECT_EQ(test, block[75], 0x10);
	KUNIT_EXPECT_EQ(test, block[93], 0x10);
	KUNIT_EXPECT_EQ(test, block[111], 0x10);
}

static void panel_edid_long_name_is_cut(struct kunit *test)
{
	u8 block[DCP_PANEL_EDID_SIZE];

	dcp_panel_edid_block(&j516_panel, "A name longer than thirteen", block);
	KUNIT_EXPECT_MEMEQ(test, &block[59], "A name longer", 13);
}

static void panel_edid_parses_as_a_drm_edid(struct kunit *test)
{
	u8 block[DCP_PANEL_EDID_SIZE];
	const struct drm_edid *edid;
	struct drm_edid_product_id id;

	dcp_panel_edid_block(&j516_panel, "Built-in", block);
	edid = drm_edid_alloc(block, sizeof(block));
	KUNIT_ASSERT_NOT_NULL(test, edid);
	KUNIT_EXPECT_TRUE(test, drm_edid_valid(edid));
	drm_edid_get_product_id(edid, &id);
	KUNIT_EXPECT_EQ(test, be16_to_cpu(id.manufacturer_name), 0x0610);
	KUNIT_EXPECT_EQ(test, le32_to_cpu(id.serial_number), 0);
	drm_edid_free(edid);
}

static struct kunit_case panel_edid_cases[] = {
	KUNIT_CASE(panel_edid_block_is_well_formed),
	KUNIT_CASE(panel_edid_block_carries_vendor_size_and_name),
	KUNIT_CASE(panel_edid_long_name_is_cut),
	KUNIT_CASE(panel_edid_parses_as_a_drm_edid),
	{}
};

static struct kunit_suite panel_edid_suite = {
	.name = "apple-panel-edid",
	.test_cases = panel_edid_cases,
};
kunit_test_suite(panel_edid_suite);

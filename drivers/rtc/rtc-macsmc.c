// SPDX-License-Identifier: GPL-2.0-only OR MIT
/*
 * Apple SMC RTC driver
 * Copyright The Asahi Linux Contributors
 */

#include <linux/bitops.h>
#include <linux/interrupt.h>
#include <linux/mfd/macsmc.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/nvmem-consumer.h>
#include <linux/of.h>
#include <linux/platform_device.h>
#include <linux/pm_wakeirq.h>
#include <linux/rtc.h>
#include <linux/slab.h>
#include <linux/unaligned.h>

/* 48-bit RTC */
#define RTC_BYTES 6
#define RTC_BITS (8 * RTC_BYTES)

/* 32768 Hz clock */
#define RTC_SEC_SHIFT 15

/* Offset kept as a 64-bit little-endian count of whole seconds */
#define RTC_OFFSET_SECS_BYTES 8

/*
 * PMU alarm control. The alarm compares a 32-bit count of whole seconds of
 * the free-running counter, i.e. CLKM >> RTC_SEC_SHIFT, without the offset.
 */
#define RTC_CTRL_MONITOR	BIT(0)
#define RTC_CTRL_ALARM_EVENT	BIT(1)	/* only without a separate event register */
#define RTC_CTRL_ALARM_EN	BIT(6)

/* The alarm's bit in the RTC IRQ mask register; 1 masks it */
#define RTC_MASK_ALARM		BIT(0)

/*
 * Optional separate alarm event register (t8103 Sera: 0xd00c), bit 0. Where
 * it exists, ctrl bit 1 is not known to mean anything and is never changed.
 */
#define RTC_EVENT_ALARM		BIT(0)

/*
 * How the separate event register is cleared has not been seen on t8103: it
 * is only read unless this is set, and then only its bit 0 is written as 1
 * (write-1-to-clear, as on T602x), and only when no other bit is set.
 */
static bool event_clear;
module_param(event_clear, bool, 0644);
MODULE_PARM_DESC(event_clear,
		 "Clear a latched alarm in the separate event register by writing 1 to bit 0 (default: only read it)");

static bool wake_from_off;
module_param(wake_from_off, bool, 0644);
MODULE_PARM_DESC(wake_from_off,
		 "Leave an alarm Linux armed armed across power-off (default: disarm it at shutdown)");

struct macsmc_rtc {
	struct device *dev;
	struct apple_smc *smc;
	struct rtc_device *rtc_dev;
	struct nvmem_cell *rtc_offset;
	bool offset_in_seconds;
	/* Alarm, if the PMU's alarm registers and interrupt are described */
	struct nvmem_cell *alarm_ctrl;
	struct nvmem_cell *alarm_value;
	struct nvmem_cell *alarm_mask;
	struct nvmem_cell *alarm_event;	/* optional */
	struct mutex alarm_lock;
	u32 alarm_secs;
	bool armed;		/* Linux armed the alarm and it has not fired */
	int alarm_irq;
};

static int macsmc_rtc_get_time_secs(struct macsmc_rtc *rtc, u64 ctr,
				    struct rtc_time *tm)
{
	void *p_off;
	size_t len;
	u64 off;

	p_off = nvmem_cell_read(rtc->rtc_offset, &len);
	if (IS_ERR(p_off))
		return PTR_ERR(p_off);
	if (len < RTC_OFFSET_SECS_BYTES) {
		kfree(p_off);
		return -EIO;
	}

	off = get_unaligned_le64(p_off);
	kfree(p_off);

	/*
	 * Nothing has been stored yet: the counter alone is the time since
	 * the PMU first powered up, not the wall-clock time.
	 */
	if (!off)
		return -EINVAL;

	rtc_time64_to_tm((time64_t)(ctr >> RTC_SEC_SHIFT) + (s64)off, tm);

	return 0;
}

static int macsmc_rtc_get_time(struct device *dev, struct rtc_time *tm)
{
	struct macsmc_rtc *rtc = dev_get_drvdata(dev);
	u64 ctr = 0, off = 0;
	time64_t now;
	void *p_off;
	size_t len;
	int ret;

	ret = apple_smc_read(rtc->smc, SMC_KEY(CLKM), &ctr, RTC_BYTES);
	if (ret < 0)
		return ret;
	if (ret != RTC_BYTES) {
		ctr = 0;
		return -EIO;
	}

	if (rtc->offset_in_seconds)
		return macsmc_rtc_get_time_secs(rtc, ctr, tm);

	p_off = nvmem_cell_read(rtc->rtc_offset, &len);
	if (IS_ERR(p_off))
		return PTR_ERR(p_off);
	if (len < RTC_BYTES) {
		kfree(p_off);
		return -EIO;
	}

	memcpy(&off, p_off, RTC_BYTES);
	kfree(p_off);

	/* Sign extend from 48 to 64 bits, then arithmetic shift right 15 bits to get seconds */
	now = sign_extend64(ctr + off, RTC_BITS - 1) >> RTC_SEC_SHIFT;
	rtc_time64_to_tm(now, tm);

	return ret;
}

static int macsmc_rtc_set_time(struct device *dev, struct rtc_time *tm)
{
	struct macsmc_rtc *rtc = dev_get_drvdata(dev);
	u64 ctr = 0, off = 0;
	int ret;

	ret = apple_smc_read(rtc->smc, SMC_KEY(CLKM), &ctr, RTC_BYTES);
	if (ret < 0)
		return ret;
	if (ret != RTC_BYTES) {
		ctr = 0;
		return -EIO;
	}

	if (rtc->offset_in_seconds) {
		__le64 off_secs;

		/*
		 * The offset holds whole seconds, so the counter's fraction of a
		 * second is lost: the clock may run up to a second ahead.
		 */
		off_secs = cpu_to_le64(rtc_tm_to_time64(tm) - (ctr >> RTC_SEC_SHIFT));
		ret = nvmem_cell_write(rtc->rtc_offset, &off_secs, sizeof(off_secs));
		return ret < 0 ? ret : 0;
	}

	/* This sets the offset such that the set second begins now */
	off = (rtc_tm_to_time64(tm) << RTC_SEC_SHIFT) - ctr;
	return nvmem_cell_write(rtc->rtc_offset, &off, RTC_BYTES);
}


static int macsmc_rtc_read_u8(struct nvmem_cell *cell, u8 *val)
{
	size_t len;
	void *p;

	p = nvmem_cell_read(cell, &len);
	if (IS_ERR(p))
		return PTR_ERR(p);
	if (len < 1) {
		kfree(p);
		return -EIO;
	}
	*val = *(u8 *)p;
	kfree(p);

	return 0;
}

static int macsmc_rtc_write_u8(struct nvmem_cell *cell, u8 val)
{
	int ret = nvmem_cell_write(cell, &val, sizeof(val));

	return ret < 0 ? ret : 0;
}

/* The wall-clock offset in whole seconds: wall time = counter seconds + offset */
static int macsmc_rtc_offset_secs(struct macsmc_rtc *rtc, s64 *secs)
{
	u64 off = 0;
	size_t len;
	void *p;

	p = nvmem_cell_read(rtc->rtc_offset, &len);
	if (IS_ERR(p))
		return PTR_ERR(p);

	if (rtc->offset_in_seconds) {
		if (len < RTC_OFFSET_SECS_BYTES) {
			kfree(p);
			return -EIO;
		}
		*secs = (s64)get_unaligned_le64(p);
	} else {
		if (len < RTC_BYTES) {
			kfree(p);
			return -EIO;
		}
		memcpy(&off, p, RTC_BYTES);
		*secs = sign_extend64(off, RTC_BITS - 1) >> RTC_SEC_SHIFT;
	}
	kfree(p);

	return 0;
}

/*
 * Read-modify-write the bits in @mask of a one-byte cell to @val, leaving
 * every other bit as read. Nothing is written when they already match.
 */
static int macsmc_rtc_update_u8(struct nvmem_cell *cell, u8 mask, u8 val)
{
	u8 old, new;
	int ret;

	ret = macsmc_rtc_read_u8(cell, &old);
	if (ret)
		return ret;
	new = (old & ~mask) | (val & mask);
	if (new == old)
		return 0;
	return macsmc_rtc_write_u8(cell, new);
}

/* The free-running counter in whole seconds, as the alarm compares it */
static int macsmc_rtc_counter_secs(struct macsmc_rtc *rtc, u32 *secs)
{
	u64 ctr = 0;
	int ret;

	ret = apple_smc_read(rtc->smc, SMC_KEY(CLKM), &ctr, RTC_BYTES);
	if (ret < 0)
		return ret;
	if (ret != RTC_BYTES)
		return -EIO;
	*secs = (u32)(ctr >> RTC_SEC_SHIFT);
	return 0;
}

/*
 * Whether the alarm is latched: bit 0 of the separate event register where
 * there is one, else bit 1 of the control byte. @ctrl and @ev get the raw
 * bytes (@ev is 0 without an event register).
 */
static int macsmc_rtc_latched(struct macsmc_rtc *rtc, u8 *ctrl, u8 *ev,
			      bool *latched)
{
	int ret;

	*ev = 0;
	ret = macsmc_rtc_read_u8(rtc->alarm_ctrl, ctrl);
	if (!ret && rtc->alarm_event)
		ret = macsmc_rtc_read_u8(rtc->alarm_event, ev);
	if (ret)
		return ret;
	*latched = rtc->alarm_event ? (*ev & RTC_EVENT_ALARM) :
				      (*ctrl & RTC_CTRL_ALARM_EVENT);
	return 0;
}

/*
 * Clear a latched alarm.
 *
 * Without an event register, the latch is ctrl bit 1, cleared by writing it
 * back as 1 (T602x/T6030); every other bit of the byte is written as read.
 *
 * With one (t8103), nothing is written unless event_clear is set, and then
 * only 0x01, only when bit 0 is the one bit set: that clears it if the
 * register is write-1-to-clear and changes nothing if it is plain read/write.
 */
static int macsmc_rtc_clear_event(struct macsmc_rtc *rtc)
{
	u8 v;
	int ret;

	if (!rtc->alarm_event) {
		ret = macsmc_rtc_read_u8(rtc->alarm_ctrl, &v);
		if (ret || !(v & RTC_CTRL_ALARM_EVENT))
			return ret;
		return macsmc_rtc_write_u8(rtc->alarm_ctrl, v);
	}

	ret = macsmc_rtc_read_u8(rtc->alarm_event, &v);
	if (ret || !(v & RTC_EVENT_ALARM) || !event_clear)
		return ret;
	if (v != RTC_EVENT_ALARM) {
		dev_warn_once(rtc->dev,
			      "event register reads %#04x, not just the alarm bit: left alone\n", v);
		return 0;
	}
	ret = macsmc_rtc_write_u8(rtc->alarm_event, RTC_EVENT_ALARM);
	if (!ret && !macsmc_rtc_read_u8(rtc->alarm_event, &v))
		dev_info(rtc->dev, "event register: wrote 0x01, reads %#04x\n", v);
	return ret;
}

/*
 * Arm the alarm at counter second @secs, or disarm it if @secs is 0. Only
 * the alarm's own bits are changed, each by read-modify-write: ctrl EN (and
 * MONITOR when arming, as the ADT's alarm settings give), bit 0 of the IRQ
 * mask, the 4-byte alarm value, and the latch as macsmc_rtc_clear_event()
 * allows. Disarming leaves MONITOR as it is.
 */
static int macsmc_rtc_program_alarm(struct macsmc_rtc *rtc, u32 secs)
{
	__le32 val = cpu_to_le32(secs);
	u8 clear_bits = rtc->alarm_event ? 0 : RTC_CTRL_ALARM_EVENT;
	int ret;

	if (!secs) {
		rtc->armed = false;
		/* Never write the legacy latch bit back as 1 here: it would clear it. */
		ret = macsmc_rtc_update_u8(rtc->alarm_ctrl,
					   RTC_CTRL_ALARM_EN | clear_bits, 0);
		if (!ret)
			ret = macsmc_rtc_update_u8(rtc->alarm_mask, RTC_MASK_ALARM,
						   RTC_MASK_ALARM);
		if (!ret)
			ret = macsmc_rtc_clear_event(rtc);
		return ret;
	}

	ret = macsmc_rtc_clear_event(rtc);
	if (ret)
		return ret;
	ret = nvmem_cell_write(rtc->alarm_value, &val, sizeof(val));
	if (ret < 0)
		return ret;
	ret = macsmc_rtc_update_u8(rtc->alarm_mask, RTC_MASK_ALARM, 0);
	if (ret)
		return ret;
	ret = macsmc_rtc_update_u8(rtc->alarm_ctrl,
				   RTC_CTRL_ALARM_EN | RTC_CTRL_MONITOR | clear_bits,
				   RTC_CTRL_ALARM_EN | RTC_CTRL_MONITOR);
	if (!ret)
		rtc->armed = true;
	return ret;
}

static int macsmc_rtc_read_alarm(struct device *dev, struct rtc_wkalrm *alrm)
{
	struct macsmc_rtc *rtc = dev_get_drvdata(dev);
	bool latched;
	size_t len;
	s64 off;
	u32 secs;
	void *p;
	u8 ctrl, ev;
	int ret;

	ret = macsmc_rtc_offset_secs(rtc, &off);
	if (ret)
		return ret;

	mutex_lock(&rtc->alarm_lock);
	ret = macsmc_rtc_latched(rtc, &ctrl, &ev, &latched);
	if (ret)
		goto out;
	p = nvmem_cell_read(rtc->alarm_value, &len);
	if (IS_ERR(p)) {
		ret = PTR_ERR(p);
		goto out;
	}
	secs = len >= sizeof(u32) ? get_unaligned_le32(p) : 0;
	kfree(p);

	rtc_time64_to_tm((time64_t)secs + off, &alrm->time);
	alrm->enabled = !!(ctrl & RTC_CTRL_ALARM_EN);
	alrm->pending = latched;
out:
	mutex_unlock(&rtc->alarm_lock);
	return ret;
}

static int macsmc_rtc_set_alarm(struct device *dev, struct rtc_wkalrm *alrm)
{
	struct macsmc_rtc *rtc = dev_get_drvdata(dev);
	s64 off, secs;
	int ret;

	ret = macsmc_rtc_offset_secs(rtc, &off);
	if (ret)
		return ret;

	/*
	 * With a tick offset, off is rounded down to whole seconds but the
	 * time is floor((ctr + off) / 2^15), so the alarm fires at the set
	 * second or up to one second after it.
	 */
	secs = rtc_tm_to_time64(&alrm->time) - off;
	if (secs <= 0 || secs > U32_MAX)
		return -ERANGE;

	mutex_lock(&rtc->alarm_lock);
	rtc->alarm_secs = secs;
	ret = macsmc_rtc_program_alarm(rtc, alrm->enabled ? rtc->alarm_secs : 0);
	mutex_unlock(&rtc->alarm_lock);

	return ret;
}

static int macsmc_rtc_alarm_irq_enable(struct device *dev, unsigned int enabled)
{
	struct macsmc_rtc *rtc = dev_get_drvdata(dev);
	int ret;

	mutex_lock(&rtc->alarm_lock);
	ret = macsmc_rtc_program_alarm(rtc, enabled ? rtc->alarm_secs : 0);
	mutex_unlock(&rtc->alarm_lock);

	return ret;
}

/*
 * The PMU's interrupt 2 may come from other RTC events, or from a latch left
 * set, so it counts as this alarm only when Linux armed it. With an event
 * register, whose behaviour on t8103 has not been seen, the alarm counts as
 * fired when its second has come (within one second), latched or not; the
 * latches are logged so a lab run shows which one the PMU sets. Without one,
 * the ctrl bit 1 latch decides, as before.
 */
static irqreturn_t macsmc_rtc_alarm_irq(int irq, void *data)
{
	struct macsmc_rtc *rtc = data;
	bool latched, fired;
	u32 now = 0;
	u8 ctrl, ev;
	int ret;

	mutex_lock(&rtc->alarm_lock);
	ret = macsmc_rtc_latched(rtc, &ctrl, &ev, &latched);
	if (!ret && rtc->alarm_event)
		ret = macsmc_rtc_counter_secs(rtc, &now);
	if (ret) {
		mutex_unlock(&rtc->alarm_lock);
		dev_err_ratelimited(rtc->dev, "alarm IRQ: PMU read failed: %d\n", ret);
		return IRQ_NONE;
	}
	if (rtc->alarm_event)
		fired = rtc->armed && (s32)(now + 1 - rtc->alarm_secs) >= 0;
	else
		fired = rtc->armed && latched;

	dev_info_ratelimited(rtc->dev,
			     "alarm IRQ: ctrl %#04x event %#04x counter %u s alarm %u s armed %d: %s\n",
			     ctrl, ev, now, rtc->alarm_secs, rtc->armed,
			     fired ? "fired" : "not this alarm");
	if (!fired) {
		mutex_unlock(&rtc->alarm_lock);
		return IRQ_NONE;
	}
	/* One-shot: disarm (and clear the latch, where allowed) until set again */
	ret = macsmc_rtc_program_alarm(rtc, 0);
	mutex_unlock(&rtc->alarm_lock);
	if (ret)
		dev_err(rtc->dev, "alarm IRQ: disarm failed: %d\n", ret);

	rtc_update_irq(rtc->rtc_dev, 1, RTC_AF | RTC_IRQF);

	return IRQ_HANDLED;
}

static const struct rtc_class_ops macsmc_rtc_ops = {
	.read_time = macsmc_rtc_get_time,
	.set_time = macsmc_rtc_set_time,
	.read_alarm = macsmc_rtc_read_alarm,
	.set_alarm = macsmc_rtc_set_alarm,
	.alarm_irq_enable = macsmc_rtc_alarm_irq_enable,
};

/* A missing cell means no alarm, not an error */
static struct nvmem_cell *macsmc_rtc_cell_optional(struct device *dev,
						   const char *id)
{
	struct nvmem_cell *cell = devm_nvmem_cell_get(dev, id);

	if (IS_ERR(cell) && PTR_ERR(cell) == -ENOENT)
		return NULL;
	return cell;
}

/*
 * Log the alarm registers as firmware left them (reads only). @armed is set
 * when the alarm's enable bit is.
 */
static int macsmc_rtc_log_firmware_state(struct macsmc_rtc *rtc, bool *armed)
{
	u8 ctrl, mask, ev = 0;
	u32 value = 0, now = 0;
	size_t len;
	void *p;
	int ret;

	ret = macsmc_rtc_read_u8(rtc->alarm_ctrl, &ctrl);
	if (!ret)
		ret = macsmc_rtc_read_u8(rtc->alarm_mask, &mask);
	if (!ret && rtc->alarm_event)
		ret = macsmc_rtc_read_u8(rtc->alarm_event, &ev);
	if (ret)
		return ret;
	p = nvmem_cell_read(rtc->alarm_value, &len);
	if (IS_ERR(p))
		return PTR_ERR(p);
	if (len >= sizeof(u32))
		value = get_unaligned_le32(p);
	kfree(p);
	macsmc_rtc_counter_secs(rtc, &now);

	if (rtc->alarm_event)
		dev_info(rtc->dev,
			 "firmware alarm state: ctrl %#04x alarm %#010x event %#04x mask %#04x, counter %u s\n",
			 ctrl, value, ev, mask, now);
	else
		dev_info(rtc->dev,
			 "firmware alarm state: ctrl %#04x alarm %#010x mask %#04x, counter %u s\n",
			 ctrl, value, mask, now);
	*armed = ctrl & RTC_CTRL_ALARM_EN;
	return 0;
}

/* The alarm is optional: it needs the PMU's alarm registers and interrupt. */
static int macsmc_rtc_init_alarm(struct platform_device *pdev,
				 struct macsmc_rtc *rtc)
{
	struct device *dev = &pdev->dev;
	bool armed_by_fw;
	int irq, ret;

	rtc->alarm_ctrl = macsmc_rtc_cell_optional(dev, "rtc_ctrl");
	rtc->alarm_value = macsmc_rtc_cell_optional(dev, "rtc_alarm");
	rtc->alarm_mask = macsmc_rtc_cell_optional(dev, "rtc_alarm_mask");
	rtc->alarm_event = macsmc_rtc_cell_optional(dev, "rtc_alarm_event");
	if (IS_ERR(rtc->alarm_event))
		return PTR_ERR(rtc->alarm_event);
	if (IS_ERR(rtc->alarm_ctrl))
		return PTR_ERR(rtc->alarm_ctrl);
	if (IS_ERR(rtc->alarm_value))
		return PTR_ERR(rtc->alarm_value);
	if (IS_ERR(rtc->alarm_mask))
		return PTR_ERR(rtc->alarm_mask);
	if (!rtc->alarm_ctrl || !rtc->alarm_value || !rtc->alarm_mask)
		return -ENODEV;

	irq = platform_get_irq_optional(pdev, 0);
	if (irq == -EPROBE_DEFER)
		return irq;
	if (irq <= 0)
		return -ENODEV;

	/* Record what firmware left, before Linux writes anything. */
	ret = macsmc_rtc_log_firmware_state(rtc, &armed_by_fw);
	if (ret)
		return ret;

	/*
	 * Firmware can leave the alarm armed. That is not an alarm Linux set,
	 * so it is disarmed before the interrupt is requested. Left disarmed,
	 * nothing is written; a latched event is only cleared as
	 * macsmc_rtc_clear_event() allows, which on t8103 is not by default.
	 */
	if (armed_by_fw) {
		ret = macsmc_rtc_program_alarm(rtc, 0);
		if (ret)
			return ret;
		dev_info(dev, "disarmed the alarm firmware left armed\n");
	}

	ret = devm_request_threaded_irq(dev, irq, NULL, macsmc_rtc_alarm_irq,
					IRQF_ONESHOT, dev_name(dev), rtc);
	if (ret)
		return ret;
	rtc->alarm_irq = irq;

	if (of_property_read_bool(dev->of_node, "wakeup-source")) {
		ret = devm_device_init_wakeup(dev);
		if (!ret)
			ret = devm_pm_set_wake_irq(dev, irq);
		if (ret)
			dev_warn(dev, "alarm cannot wake the system: %d\n", ret);
	}

	dev_info(dev, "PMU alarm on IRQ %d%s%s%s\n", irq,
		 rtc->alarm_event ? ", with event register" : "",
		 rtc->alarm_event && !event_clear ? " (read only)" : "",
		 device_may_wakeup(dev) ? ", wake source" : "");

	return 0;
}

/*
 * An armed alarm that is meant to wake the system must be able to: if its
 * interrupt cannot be made a wake source (the SPMI controller's interrupt
 * was found dead), fail the suspend rather than sleep with no wake.
 */
static int macsmc_rtc_suspend(struct device *dev)
{
	struct macsmc_rtc *rtc = dev_get_drvdata(dev);
	bool armed;
	int ret;

	if (!rtc->alarm_irq || !device_may_wakeup(dev))
		return 0;
	mutex_lock(&rtc->alarm_lock);
	armed = rtc->armed;
	mutex_unlock(&rtc->alarm_lock);
	if (!armed)
		return 0;

	ret = enable_irq_wake(rtc->alarm_irq);
	if (ret) {
		dev_err(dev, "alarm armed but its interrupt cannot wake the system (%d): not suspending\n",
			ret);
		return ret;
	}
	disable_irq_wake(rtc->alarm_irq);
	return 0;
}

static DEFINE_SIMPLE_DEV_PM_OPS(macsmc_rtc_pm_ops, macsmc_rtc_suspend, NULL);

/* An alarm Linux armed is disarmed at power-off, unless wake_from_off is set. */
static void macsmc_rtc_shutdown(struct platform_device *pdev)
{
	struct macsmc_rtc *rtc = platform_get_drvdata(pdev);

	if (!rtc || !rtc->alarm_irq || wake_from_off)
		return;
	mutex_lock(&rtc->alarm_lock);
	if (rtc->armed)
		macsmc_rtc_program_alarm(rtc, 0);
	mutex_unlock(&rtc->alarm_lock);
}

static int macsmc_rtc_probe(struct platform_device *pdev)
{
	struct apple_smc *smc = dev_get_drvdata(pdev->dev.parent);
	struct macsmc_rtc *rtc;
	int ret;

	/*
	 * MFD will probe this device even without a node in the device tree,
	 * thus bail out early if the SMC on the current machines does not
	 * support RTC and has no node in the device tree.
	 */
	if (!pdev->dev.of_node)
		return -ENODEV;

	rtc = devm_kzalloc(&pdev->dev, sizeof(*rtc), GFP_KERNEL);
	if (!rtc)
		return -ENOMEM;

	rtc->dev = &pdev->dev;
	rtc->smc = smc;
	mutex_init(&rtc->alarm_lock);
	platform_set_drvdata(pdev, rtc);

	rtc->rtc_offset = devm_nvmem_cell_get(&pdev->dev, "rtc_offset");
	if (IS_ERR(rtc->rtc_offset))
		return dev_err_probe(&pdev->dev, PTR_ERR(rtc->rtc_offset),
				     "Failed to get rtc_offset NVMEM cell\n");

	rtc->rtc_dev = devm_rtc_allocate_device(&pdev->dev);
	if (IS_ERR(rtc->rtc_dev))
		return PTR_ERR(rtc->rtc_dev);

	rtc->offset_in_seconds = of_property_read_bool(pdev->dev.of_node,
						       "apple,rtc-offset-seconds");

	rtc->rtc_dev->ops = &macsmc_rtc_ops;
	ret = macsmc_rtc_init_alarm(pdev, rtc);
	if (ret == -EPROBE_DEFER)
		return ret;
	if (ret) {
		if (ret != -ENODEV)
			dev_warn(&pdev->dev, "no alarm: %d\n", ret);
		clear_bit(RTC_FEATURE_ALARM, rtc->rtc_dev->features);
	}
	if (rtc->offset_in_seconds) {
		rtc->rtc_dev->range_min = 0;
		rtc->rtc_dev->range_max = RTC_TIMESTAMP_END_9999;
	} else {
		rtc->rtc_dev->range_min = S64_MIN >> (RTC_SEC_SHIFT + (64 - RTC_BITS));
		rtc->rtc_dev->range_max = S64_MAX >> (RTC_SEC_SHIFT + (64 - RTC_BITS));
	}

	return devm_rtc_register_device(rtc->rtc_dev);
}

static const struct of_device_id macsmc_rtc_of_table[] = {
	{ .compatible = "apple,smc-rtc", },
	{}
};
MODULE_DEVICE_TABLE(of, macsmc_rtc_of_table);

static struct platform_driver macsmc_rtc_driver = {
	.driver = {
		.name = "macsmc-rtc",
		.of_match_table = macsmc_rtc_of_table,
		.pm = pm_sleep_ptr(&macsmc_rtc_pm_ops),
	},
	.probe = macsmc_rtc_probe,
	.shutdown = macsmc_rtc_shutdown,
};
module_platform_driver(macsmc_rtc_driver);

MODULE_LICENSE("Dual MIT/GPL");
MODULE_DESCRIPTION("Apple SMC RTC driver");
MODULE_AUTHOR("Hector Martin <marcan@marcan.st>");

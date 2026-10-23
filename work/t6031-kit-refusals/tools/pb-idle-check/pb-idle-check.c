// SPDX-License-Identifier: MIT
/*
 * pb-idle-check: one Vulkan device (one DRM VM, one client) runs a step list of heavy renders,
 * compute dispatches and idle gaps, to test the G15 PB re-initialization and reclaim paths on
 * hardware with a single client:
 *
 *   pb-idle-check [-n TRIANGLES] [-z SIZE] [-x CMD] STEP...
 *     R       one render pass: clear, then TRIANGLES triangles of SIZE (NDC units) spread over a
 *             2048x2048 target, read back; checks that sampled pixels were drawn
 *     C       one compute dispatch writing 65536 words; checks every word
 *     <ms>    stay idle that long (nothing submitted)
 *   -x CMD    run CMD (a shell command) after every step, e.g. to print the PB stats line
 *
 * Every step waits for its queue to go idle before the next one. Prints one line per step with
 * the step, its wall time and its result; exits non-zero if a check or a Vulkan call failed.
 * Shaders: pb-idle-check.vert/.frag/.comp compiled to SPIR-V next to the binary.
 */
#define VK_NO_PROTOTYPES
#include <vulkan/vulkan.h>
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <libgen.h>
#include <limits.h>

#define W 2048
#define H 2048
#define CWORDS 65536

static PFN_vkGetInstanceProcAddr gipa;
#define F(name) static PFN_##name name
F(vkCreateInstance); F(vkEnumeratePhysicalDevices); F(vkGetPhysicalDeviceQueueFamilyProperties);
F(vkGetPhysicalDeviceMemoryProperties); F(vkCreateDevice); F(vkGetDeviceQueue);
F(vkCreateImage); F(vkGetImageMemoryRequirements); F(vkAllocateMemory); F(vkBindImageMemory);
F(vkCreateImageView); F(vkCreateRenderPass); F(vkCreateFramebuffer); F(vkCreateShaderModule);
F(vkCreatePipelineLayout); F(vkCreateGraphicsPipelines); F(vkCreateComputePipelines);
F(vkCreateCommandPool); F(vkAllocateCommandBuffers); F(vkBeginCommandBuffer);
F(vkEndCommandBuffer); F(vkCmdBeginRenderPass); F(vkCmdEndRenderPass); F(vkCmdBindPipeline);
F(vkCmdPushConstants); F(vkCmdDraw); F(vkCmdDispatch); F(vkCmdCopyImageToBuffer);
F(vkCmdPipelineBarrier); F(vkQueueSubmit); F(vkQueueWaitIdle); F(vkResetCommandBuffer);
F(vkCreateBuffer); F(vkGetBufferMemoryRequirements); F(vkBindBufferMemory); F(vkMapMemory);
F(vkCreateDescriptorSetLayout); F(vkCreateDescriptorPool); F(vkAllocateDescriptorSets);
F(vkUpdateDescriptorSets); F(vkCmdBindDescriptorSets);

static VkInstance inst;
static VkDevice dev;
static VkPhysicalDevice phys;
static VkQueue queue;
static VkCommandBuffer cmd;
static VkPhysicalDeviceMemoryProperties memprops;

#define CK(x) do { VkResult r_ = (x); if (r_ != VK_SUCCESS) { \
	fprintf(stderr, "%s:%d: %s = %d\n", __FILE__, __LINE__, #x, r_); exit(2); } } while (0)

static double now_ms(void)
{
	struct timespec t;

	clock_gettime(CLOCK_MONOTONIC, &t);
	return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

static void load_instance(void)
{
#define L(name) name = (PFN_##name)gipa(inst, #name); if (!name) { fprintf(stderr, "no %s\n", #name); exit(2); }
	L(vkEnumeratePhysicalDevices); L(vkGetPhysicalDeviceQueueFamilyProperties);
	L(vkGetPhysicalDeviceMemoryProperties); L(vkCreateDevice); L(vkGetDeviceQueue);
	L(vkCreateImage); L(vkGetImageMemoryRequirements); L(vkAllocateMemory);
	L(vkBindImageMemory); L(vkCreateImageView); L(vkCreateRenderPass); L(vkCreateFramebuffer);
	L(vkCreateShaderModule); L(vkCreatePipelineLayout); L(vkCreateGraphicsPipelines);
	L(vkCreateComputePipelines); L(vkCreateCommandPool); L(vkAllocateCommandBuffers);
	L(vkBeginCommandBuffer); L(vkEndCommandBuffer); L(vkCmdBeginRenderPass);
	L(vkCmdEndRenderPass); L(vkCmdBindPipeline); L(vkCmdPushConstants); L(vkCmdDraw);
	L(vkCmdDispatch); L(vkCmdCopyImageToBuffer); L(vkCmdPipelineBarrier); L(vkQueueSubmit);
	L(vkQueueWaitIdle); L(vkResetCommandBuffer); L(vkCreateBuffer);
	L(vkGetBufferMemoryRequirements); L(vkBindBufferMemory); L(vkMapMemory);
	L(vkCreateDescriptorSetLayout); L(vkCreateDescriptorPool); L(vkAllocateDescriptorSets);
	L(vkUpdateDescriptorSets); L(vkCmdBindDescriptorSets);
#undef L
}

static uint32_t memtype(uint32_t bits, VkMemoryPropertyFlags want)
{
	for (uint32_t i = 0; i < memprops.memoryTypeCount; i++)
		if ((bits & (1u << i)) && (memprops.memoryTypes[i].propertyFlags & want) == want)
			return i;
	fprintf(stderr, "no memory type %#x for %#x\n", want, bits);
	exit(2);
}

static VkDeviceMemory alloc_for(VkMemoryRequirements req, VkMemoryPropertyFlags want)
{
	VkMemoryAllocateInfo ai = {
		.sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
		.allocationSize = req.size,
		.memoryTypeIndex = memtype(req.memoryTypeBits, want),
	};
	VkDeviceMemory m;

	CK(vkAllocateMemory(dev, &ai, NULL, &m));
	return m;
}

static VkBuffer host_buffer(VkDeviceSize size, VkBufferUsageFlags usage, void **map)
{
	VkBufferCreateInfo bi = {
		.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
		.size = size, .usage = usage, .sharingMode = VK_SHARING_MODE_EXCLUSIVE,
	};
	VkBuffer b;
	VkMemoryRequirements req;
	VkDeviceMemory m;

	CK(vkCreateBuffer(dev, &bi, NULL, &b));
	vkGetBufferMemoryRequirements(dev, b, &req);
	m = alloc_for(req, VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT);
	CK(vkBindBufferMemory(dev, b, m, 0));
	CK(vkMapMemory(dev, m, 0, size, 0, map));
	return b;
}

static VkShaderModule shader(const char *dir, const char *name)
{
	char path[PATH_MAX];
	FILE *f;
	long n;
	uint32_t *code;
	VkShaderModule sm;

	snprintf(path, sizeof(path), "%s/%s", dir, name);
	f = fopen(path, "rb");
	if (!f) {
		perror(path);
		exit(2);
	}
	fseek(f, 0, SEEK_END);
	n = ftell(f);
	fseek(f, 0, SEEK_SET);
	code = malloc(n);
	if (fread(code, 1, n, f) != (size_t)n)
		exit(2);
	fclose(f);
	VkShaderModuleCreateInfo ci = {
		.sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO, .codeSize = n, .pCode = code,
	};
	CK(vkCreateShaderModule(dev, &ci, NULL, &sm));
	return sm;
}

static void submit_wait(void)
{
	VkSubmitInfo si = {
		.sType = VK_STRUCTURE_TYPE_SUBMIT_INFO, .commandBufferCount = 1, .pCommandBuffers = &cmd,
	};

	CK(vkQueueSubmit(queue, 1, &si, VK_NULL_HANDLE));
	CK(vkQueueWaitIdle(queue));
}

static void begin(void)
{
	VkCommandBufferBeginInfo bi = {
		.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
		.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT,
	};

	CK(vkResetCommandBuffer(cmd, 0));
	CK(vkBeginCommandBuffer(cmd, &bi));
}

int main(int argc, char **argv)
{
	uint32_t triangles = 200000;
	float size = 0.25f;
	const char *after = NULL;
	int opt, failed = 0;
	char exe[PATH_MAX], *dir;
	void *lib;

	while ((opt = getopt(argc, argv, "n:z:x:")) != -1) {
		switch (opt) {
		case 'n': triangles = strtoul(optarg, NULL, 0); break;
		case 'z': size = strtof(optarg, NULL); break;
		case 'x': after = optarg; break;
		default:
			fprintf(stderr, "usage: %s [-n TRIANGLES] [-z SIZE] [-x CMD] STEP...\n", argv[0]);
			return 2;
		}
	}
	if (optind >= argc) {
		fprintf(stderr, "no steps\n");
		return 2;
	}
	if (!realpath("/proc/self/exe", exe))
		return 2;
	dir = dirname(exe);

	lib = dlopen("libvulkan.so.1", RTLD_NOW);
	if (!lib) {
		fprintf(stderr, "%s\n", dlerror());
		return 2;
	}
	gipa = (PFN_vkGetInstanceProcAddr)dlsym(lib, "vkGetInstanceProcAddr");
	vkCreateInstance = (PFN_vkCreateInstance)gipa(NULL, "vkCreateInstance");
	VkApplicationInfo app = {
		.sType = VK_STRUCTURE_TYPE_APPLICATION_INFO, .pApplicationName = "pb-idle-check",
		.apiVersion = VK_API_VERSION_1_1,
	};
	VkInstanceCreateInfo ici = {
		.sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO, .pApplicationInfo = &app,
	};
	CK(vkCreateInstance(&ici, NULL, &inst));
	load_instance();
	uint32_t n = 1;
	CK(vkEnumeratePhysicalDevices(inst, &n, &phys));
	vkGetPhysicalDeviceMemoryProperties(phys, &memprops);
	uint32_t nq = 0;
	vkGetPhysicalDeviceQueueFamilyProperties(phys, &nq, NULL);
	VkQueueFamilyProperties qp[8];
	nq = nq > 8 ? 8 : nq;
	vkGetPhysicalDeviceQueueFamilyProperties(phys, &nq, qp);
	uint32_t qf = UINT32_MAX;
	for (uint32_t i = 0; i < nq; i++)
		if ((qp[i].queueFlags & (VK_QUEUE_GRAPHICS_BIT | VK_QUEUE_COMPUTE_BIT)) ==
		    (VK_QUEUE_GRAPHICS_BIT | VK_QUEUE_COMPUTE_BIT)) {
			qf = i;
			break;
		}
	if (qf == UINT32_MAX) {
		fprintf(stderr, "no graphics+compute queue\n");
		return 2;
	}
	float prio = 1.0f;
	VkDeviceQueueCreateInfo qci = {
		.sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO, .queueFamilyIndex = qf,
		.queueCount = 1, .pQueuePriorities = &prio,
	};
	VkDeviceCreateInfo dci = {
		.sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO, .queueCreateInfoCount = 1,
		.pQueueCreateInfos = &qci,
	};
	CK(vkCreateDevice(phys, &dci, NULL, &dev));
	vkGetDeviceQueue(dev, qf, 0, &queue);

	/* Render target, render pass, framebuffer, graphics pipeline. */
	VkImageCreateInfo imci = {
		.sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO, .imageType = VK_IMAGE_TYPE_2D,
		.format = VK_FORMAT_R8G8B8A8_UNORM, .extent = { W, H, 1 }, .mipLevels = 1,
		.arrayLayers = 1, .samples = VK_SAMPLE_COUNT_1_BIT, .tiling = VK_IMAGE_TILING_OPTIMAL,
		.usage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_TRANSFER_SRC_BIT,
		.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED,
	};
	VkImage img;
	VkMemoryRequirements req;
	CK(vkCreateImage(dev, &imci, NULL, &img));
	vkGetImageMemoryRequirements(dev, img, &req);
	CK(vkBindImageMemory(dev, img, alloc_for(req, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT), 0));
	VkImageViewCreateInfo vci = {
		.sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO, .image = img,
		.viewType = VK_IMAGE_VIEW_TYPE_2D, .format = VK_FORMAT_R8G8B8A8_UNORM,
		.subresourceRange = { VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1 },
	};
	VkImageView view;
	CK(vkCreateImageView(dev, &vci, NULL, &view));
	VkAttachmentDescription att = {
		.format = VK_FORMAT_R8G8B8A8_UNORM, .samples = VK_SAMPLE_COUNT_1_BIT,
		.loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR, .storeOp = VK_ATTACHMENT_STORE_OP_STORE,
		.stencilLoadOp = VK_ATTACHMENT_LOAD_OP_DONT_CARE,
		.stencilStoreOp = VK_ATTACHMENT_STORE_OP_DONT_CARE,
		.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED,
		.finalLayout = VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
	};
	VkAttachmentReference ref = { 0, VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL };
	VkSubpassDescription sub = {
		.pipelineBindPoint = VK_PIPELINE_BIND_POINT_GRAPHICS, .colorAttachmentCount = 1,
		.pColorAttachments = &ref,
	};
	VkRenderPassCreateInfo rpci = {
		.sType = VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO, .attachmentCount = 1,
		.pAttachments = &att, .subpassCount = 1, .pSubpasses = &sub,
	};
	VkRenderPass rp;
	CK(vkCreateRenderPass(dev, &rpci, NULL, &rp));
	VkFramebufferCreateInfo fci = {
		.sType = VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO, .renderPass = rp,
		.attachmentCount = 1, .pAttachments = &view, .width = W, .height = H, .layers = 1,
	};
	VkFramebuffer fb;
	CK(vkCreateFramebuffer(dev, &fci, NULL, &fb));
	VkPushConstantRange pcr = { VK_SHADER_STAGE_VERTEX_BIT, 0, 8 };
	VkPipelineLayoutCreateInfo plci = {
		.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO, .pushConstantRangeCount = 1,
		.pPushConstantRanges = &pcr,
	};
	VkPipelineLayout gl;
	CK(vkCreatePipelineLayout(dev, &plci, NULL, &gl));
	VkPipelineShaderStageCreateInfo stages[2] = {
		{ .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
		  .stage = VK_SHADER_STAGE_VERTEX_BIT, .module = shader(dir, "pb-idle-check.vert.spv"),
		  .pName = "main" },
		{ .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
		  .stage = VK_SHADER_STAGE_FRAGMENT_BIT, .module = shader(dir, "pb-idle-check.frag.spv"),
		  .pName = "main" },
	};
	VkPipelineVertexInputStateCreateInfo vis = {
		.sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
	};
	VkPipelineInputAssemblyStateCreateInfo ias = {
		.sType = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
		.topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
	};
	VkViewport vp = { 0, 0, W, H, 0, 1 };
	VkRect2D sc = { { 0, 0 }, { W, H } };
	VkPipelineViewportStateCreateInfo vps = {
		.sType = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO, .viewportCount = 1,
		.pViewports = &vp, .scissorCount = 1, .pScissors = &sc,
	};
	VkPipelineRasterizationStateCreateInfo rs = {
		.sType = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
		.polygonMode = VK_POLYGON_MODE_FILL, .cullMode = VK_CULL_MODE_NONE, .lineWidth = 1,
	};
	VkPipelineMultisampleStateCreateInfo ms = {
		.sType = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
		.rasterizationSamples = VK_SAMPLE_COUNT_1_BIT,
	};
	VkPipelineColorBlendAttachmentState cba = { .colorWriteMask = 0xf };
	VkPipelineColorBlendStateCreateInfo cbs = {
		.sType = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
		.attachmentCount = 1, .pAttachments = &cba,
	};
	VkGraphicsPipelineCreateInfo gpci = {
		.sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO, .stageCount = 2,
		.pStages = stages, .pVertexInputState = &vis, .pInputAssemblyState = &ias,
		.pViewportState = &vps, .pRasterizationState = &rs, .pMultisampleState = &ms,
		.pColorBlendState = &cbs, .layout = gl, .renderPass = rp,
	};
	VkPipeline gp;
	CK(vkCreateGraphicsPipelines(dev, VK_NULL_HANDLE, 1, &gpci, NULL, &gp));

	/* Compute pipeline: one storage buffer. */
	void *cmap;
	VkBuffer cbuf = host_buffer(CWORDS * 4, VK_BUFFER_USAGE_STORAGE_BUFFER_BIT, &cmap);
	VkDescriptorSetLayoutBinding dslb = {
		0, VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 1, VK_SHADER_STAGE_COMPUTE_BIT, NULL,
	};
	VkDescriptorSetLayoutCreateInfo dslci = {
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO, .bindingCount = 1,
		.pBindings = &dslb,
	};
	VkDescriptorSetLayout dsl;
	CK(vkCreateDescriptorSetLayout(dev, &dslci, NULL, &dsl));
	VkDescriptorPoolSize dps = { VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, 1 };
	VkDescriptorPoolCreateInfo dpci = {
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO, .maxSets = 1,
		.poolSizeCount = 1, .pPoolSizes = &dps,
	};
	VkDescriptorPool dp;
	CK(vkCreateDescriptorPool(dev, &dpci, NULL, &dp));
	VkDescriptorSetAllocateInfo dsai = {
		.sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO, .descriptorPool = dp,
		.descriptorSetCount = 1, .pSetLayouts = &dsl,
	};
	VkDescriptorSet ds;
	CK(vkAllocateDescriptorSets(dev, &dsai, &ds));
	VkDescriptorBufferInfo dbi = { cbuf, 0, VK_WHOLE_SIZE };
	VkWriteDescriptorSet wds = {
		.sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET, .dstSet = ds, .descriptorCount = 1,
		.descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .pBufferInfo = &dbi,
	};
	vkUpdateDescriptorSets(dev, 1, &wds, 0, NULL);
	VkPushConstantRange cpcr = { VK_SHADER_STAGE_COMPUTE_BIT, 0, 4 };
	VkPipelineLayoutCreateInfo cplci = {
		.sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO, .setLayoutCount = 1,
		.pSetLayouts = &dsl, .pushConstantRangeCount = 1, .pPushConstantRanges = &cpcr,
	};
	VkPipelineLayout cl;
	CK(vkCreatePipelineLayout(dev, &cplci, NULL, &cl));
	VkComputePipelineCreateInfo cpci = {
		.sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
		.stage = { .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
			   .stage = VK_SHADER_STAGE_COMPUTE_BIT,
			   .module = shader(dir, "pb-idle-check.comp.spv"), .pName = "main" },
		.layout = cl,
	};
	VkPipeline cp;
	CK(vkCreateComputePipelines(dev, VK_NULL_HANDLE, 1, &cpci, NULL, &cp));

	void *rmap;
	VkBuffer rbuf = host_buffer((VkDeviceSize)W * H * 4, VK_BUFFER_USAGE_TRANSFER_DST_BIT, &rmap);
	VkCommandPoolCreateInfo cpoolci = {
		.sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
		.flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT, .queueFamilyIndex = qf,
	};
	VkCommandPool pool;
	CK(vkCreateCommandPool(dev, &cpoolci, NULL, &pool));
	VkCommandBufferAllocateInfo cbai = {
		.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO, .commandPool = pool,
		.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY, .commandBufferCount = 1,
	};
	CK(vkAllocateCommandBuffers(dev, &cbai, &cmd));

	printf("pb-idle-check: %u triangles of %.3f, %dx%d, queue family %u\n", triangles, size, W, H, qf);
	fflush(stdout);
	uint32_t seq = 0;
	for (int i = optind; i < argc; i++) {
		const char *step = argv[i];
		double t0 = now_ms();
		char result[96] = "ok";

		if (!strcmp(step, "R")) {
			struct { uint32_t seed; float size; } pc = { ++seq, size };
			VkClearValue clear = { .color = { .float32 = { 0, 0, 0, 1 } } };
			VkRenderPassBeginInfo rpbi = {
				.sType = VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO, .renderPass = rp,
				.framebuffer = fb, .renderArea = sc, .clearValueCount = 1,
				.pClearValues = &clear,
			};
			VkBufferImageCopy copy = {
				.imageSubresource = { VK_IMAGE_ASPECT_COLOR_BIT, 0, 0, 1 },
				.imageExtent = { W, H, 1 },
			};
			begin();
			vkCmdBeginRenderPass(cmd, &rpbi, VK_SUBPASS_CONTENTS_INLINE);
			vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_GRAPHICS, gp);
			vkCmdPushConstants(cmd, gl, VK_SHADER_STAGE_VERTEX_BIT, 0, sizeof(pc), &pc);
			vkCmdDraw(cmd, triangles * 3, 1, 0, 0);
			vkCmdEndRenderPass(cmd);
			vkCmdCopyImageToBuffer(cmd, img, VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL, rbuf, 1, &copy);
			CK(vkEndCommandBuffer(cmd));
			submit_wait();
			const uint32_t *px = rmap;
			uint64_t drawn = 0;
			for (uint64_t p = 0; p < (uint64_t)W * H; p += 97)
				drawn += px[p] != 0xff000000u;
			uint64_t samples = ((uint64_t)W * H + 96) / 97;
			if (drawn == 0) {
				failed = 1;
				snprintf(result, sizeof(result), "FAIL: %llu of %llu sampled pixels drawn",
					 (unsigned long long)drawn, (unsigned long long)samples);
			} else {
				snprintf(result, sizeof(result), "ok: %llu of %llu sampled pixels drawn",
					 (unsigned long long)drawn, (unsigned long long)samples);
			}
		} else if (!strcmp(step, "C")) {
			uint32_t key = ++seq * 0x9e3779b9u;
			memset(cmap, 0, CWORDS * 4);
			begin();
			vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, cp);
			vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, cl, 0, 1, &ds, 0, NULL);
			vkCmdPushConstants(cmd, cl, VK_SHADER_STAGE_COMPUTE_BIT, 0, 4, &key);
			vkCmdDispatch(cmd, CWORDS / 64, 1, 1);
			CK(vkEndCommandBuffer(cmd));
			submit_wait();
			const uint32_t *w = cmap;
			uint32_t bad = 0;
			for (uint32_t k = 0; k < CWORDS; k++)
				bad += w[k] != (k ^ key);
			if (bad) {
				failed = 1;
				snprintf(result, sizeof(result), "FAIL: %u of %u words wrong", bad, CWORDS);
			}
		} else {
			char *end;
			long ms = strtol(step, &end, 10);

			if (*end || ms < 0) {
				fprintf(stderr, "bad step %s\n", step);
				return 2;
			}
			usleep(ms * 1000);
			snprintf(result, sizeof(result), "idle");
		}
		printf("step %d %s: %.1f ms, %s\n", i - optind, step, now_ms() - t0, result);
		fflush(stdout);
		if (after && system(after) != 0)
			fprintf(stderr, "after-step command failed\n");
	}
	printf("pb-idle-check: %s\n", failed ? "FAIL" : "PASS");
	return failed;
}

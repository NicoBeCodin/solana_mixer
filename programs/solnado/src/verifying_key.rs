use groth16_solana::groth16::Groth16Verifyingkey;


//For a depth 8 pool
pub const VERIFYINGKEY: Groth16Verifyingkey =  Groth16Verifyingkey {
	nr_pubinputs: 3,

	vk_alpha_g1: [
		23,102,134,60,41,243,191,5,223,124,74,247,223,64,27,92,217,29,216,177,181,202,116,21,246,159,132,193,250,18,235,175,
		28,165,218,158,109,222,189,255,30,210,92,78,205,166,246,120,82,64,47,165,202,150,107,1,210,112,174,145,202,117,50,205,
	],

	vk_beta_g2: [
		18,102,54,167,113,122,211,220,246,55,133,55,197,59,233,211,20,63,240,90,134,215,199,123,190,29,5,161,15,186,102,0,
		43,8,190,111,73,170,175,78,183,184,72,9,22,43,135,17,135,40,164,168,229,156,246,53,124,185,239,32,254,70,29,178,
		11,216,35,0,18,244,42,228,150,67,75,163,178,221,64,84,120,70,180,166,64,134,189,212,127,108,116,15,12,248,138,108,
		47,241,73,74,177,137,36,81,112,229,32,17,13,195,46,176,163,180,219,158,184,59,13,158,38,202,41,126,127,225,185,85,
	],

	vk_gamme_g2: [
		25,142,147,147,146,13,72,58,114,96,191,183,49,251,93,37,241,170,73,51,53,169,231,18,151,228,133,183,174,243,18,194,
		24,0,222,239,18,31,30,118,66,106,0,102,94,92,68,121,103,67,34,212,247,94,218,221,70,222,189,92,217,146,246,237,
		9,6,137,208,88,95,240,117,236,158,153,173,105,12,51,149,188,75,49,51,112,179,142,243,85,172,218,220,209,34,151,91,
		18,200,94,165,219,140,109,235,74,171,113,128,141,203,64,143,227,209,231,105,12,67,211,123,76,230,204,1,102,250,125,170,
	],

	vk_delta_g2: [
		31,124,218,89,3,55,184,6,63,182,241,38,200,231,220,83,189,166,99,16,46,85,142,6,97,144,37,136,85,208,247,42,
		1,213,55,28,149,238,96,193,232,162,219,183,12,173,218,187,167,154,230,207,209,217,169,168,72,197,6,186,118,214,22,253,
		8,85,171,184,184,249,17,123,15,111,77,238,76,57,155,15,24,84,118,21,159,90,195,147,67,111,179,48,145,146,224,120,
		32,91,170,48,187,192,143,49,206,121,59,146,187,32,188,117,239,240,102,202,235,135,190,131,214,89,252,52,157,5,155,171,
	],

	vk_ic: &[
		[
			21,60,171,51,108,163,156,12,182,22,159,1,91,143,5,219,177,63,7,111,77,39,226,189,229,33,133,133,76,113,96,103,
			14,90,2,66,137,10,197,138,173,71,136,237,161,148,12,211,244,175,127,146,69,131,127,175,255,253,39,71,40,203,195,136,
		],
		[
			2,52,22,112,242,61,78,26,154,202,85,249,114,52,254,145,197,123,130,107,36,58,205,211,223,174,11,249,24,174,33,54,
			39,41,141,116,105,116,138,5,21,11,253,171,34,7,147,147,176,111,134,174,37,8,32,204,2,64,107,202,85,117,36,180,
		],
		[
			10,234,173,17,127,76,150,229,243,211,123,225,41,206,238,50,114,66,240,212,152,61,67,57,38,50,135,173,141,117,181,158,
			16,103,2,14,161,244,156,229,117,37,38,111,90,125,42,116,188,68,209,186,160,189,46,34,176,37,12,28,124,115,121,79,
		],
	]
};
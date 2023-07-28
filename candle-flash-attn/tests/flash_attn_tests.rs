use anyhow::Result;
use candle::{DType, Device, IndexOp, Tensor, D};

fn to_vec3_round(t: Tensor, digits: i32) -> Result<Vec<Vec<Vec<f32>>>> {
    let b = 10f32.powi(digits);
    let t = t.to_vec3::<f32>()?;
    let t = t
        .iter()
        .map(|t| {
            t.iter()
                .map(|t| t.iter().map(|t| f32::round(t * b) / b).collect())
                .collect()
        })
        .collect();
    Ok(t)
}

fn fa_acausal(q: &Tensor, k: &Tensor, v: &Tensor, softmax_scale: f32) -> Result<Tensor> {
    let in_dtype = q.dtype();
    let q = q.to_dtype(DType::F32)?;
    let k = k.to_dtype(DType::F32)?;
    let v = v.to_dtype(DType::F32)?;
    let att = (q.matmul(&k.t()?)? * softmax_scale as f64)?;
    let att = candle_nn::ops::softmax(&att, D::Minus1)?;
    // Convert to contiguous as matmul doesn't support strided vs for now.
    let output = att.matmul(&v.contiguous()?)?.to_dtype(in_dtype)?;
    Ok(output)
}

#[test]
fn flash_attn_acausal() -> Result<()> {
    let device = Device::new_cuda(0)?;
    let q = Tensor::arange(0u32, 48, &device)?
        .to_dtype(DType::F16)?
        .reshape((1, 3, 2, 8))?;
    let k = (&q / 40.)?;
    let v = (&q / 50.)?;
    let q = (&q / 30.)?;

    let ys1 = fa_acausal(&q, &k, &v, 0.5)?;
    let ys1 = ys1.i(0)?.to_dtype(DType::F32)?;
    let ys2 = {
        let q = q.transpose(1, 2)?;
        let k = k.transpose(1, 2)?;
        let v = v.transpose(1, 2)?;
        candle_flash_attn::flash_attn(&q, &k, &v, 0.5, false)?.transpose(1, 2)?
    };
    let ys2 = ys2.i(0)?.to_dtype(DType::F32)?;
    let diff = ys1.sub(&ys2)?.abs()?.flatten_all()?.max(0)?;

    assert_eq!(ys1.dims(), &[3, 2, 8]);
    assert_eq!(
        to_vec3_round(ys1, 4)?,
        &[
            [
                [0.0837, 0.1038, 0.1238, 0.1438, 0.1637, 0.1837, 0.2037, 0.2238],
                [0.0922, 0.1122, 0.1322, 0.1522, 0.1721, 0.1921, 0.2122, 0.2322]
            ],
            [
                [0.4204, 0.4404, 0.4604, 0.4805, 0.5005, 0.5205, 0.5405, 0.5605],
                [0.428, 0.448, 0.468, 0.488, 0.5083, 0.5283, 0.5483, 0.5684]
            ],
            [
                [0.7554, 0.7754, 0.7954, 0.8154, 0.8354, 0.8555, 0.8755, 0.8955],
                [0.7622, 0.7822, 0.8022, 0.8223, 0.8423, 0.8623, 0.8823, 0.9023]
            ]
        ]
    );

    assert_eq!(ys2.dims(), &[3, 2, 8]);
    assert_eq!(
        to_vec3_round(ys2, 4)?,
        &[
            [
                [0.0837, 0.1038, 0.1238, 0.1438, 0.1637, 0.1837, 0.2037, 0.2238],
                [0.0922, 0.1122, 0.1322, 0.1522, 0.1721, 0.1921, 0.2122, 0.2322]
            ],
            [
                [0.4204, 0.4404, 0.4604, 0.4805, 0.5005, 0.5205, 0.5405, 0.5605],
                [0.428, 0.448, 0.468, 0.488, 0.5083, 0.5283, 0.5483, 0.5684]
            ],
            [
                [0.7554, 0.7754, 0.7954, 0.8154, 0.8354, 0.8555, 0.8755, 0.8955],
                [0.7622, 0.7822, 0.8022, 0.8223, 0.8423, 0.8623, 0.8823, 0.9023]
            ]
        ]
    );
    assert!(diff.to_vec0::<f32>()?.abs() < 1e-5);
    Ok(())
}
